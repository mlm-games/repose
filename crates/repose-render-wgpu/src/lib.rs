use std::collections::HashMap;
use std::sync::Arc;
use std::{borrow::Cow, sync::Once};

use repose_core::{Brush, GlyphRasterConfig, RenderBackend, Scene, SceneNode, Transform};
use std::panic::{AssertUnwindSafe, catch_unwind};
use wgpu::Instance;

static ROT_WARN_ONCE: Once = Once::new();

#[derive(Clone)]
struct UploadRing {
    buf: wgpu::Buffer,
    cap: u64,
    head: u64,
}

impl UploadRing {
    fn new(device: &wgpu::Device, label: &str, cap: u64) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buf, cap, head: 0 }
    }

    fn reset(&mut self) {
        self.head = 0;
    }

    fn grow_to_fit(&mut self, device: &wgpu::Device, needed: u64) {
        if needed <= self.cap {
            return;
        }
        let new_cap = needed.next_power_of_two();
        self.buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("upload ring (grown)"),
            size: new_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.cap = new_cap;
        self.head = 0;
    }

    fn alloc_write(&mut self, queue: &wgpu::Queue, bytes: &[u8]) -> (u64, u64) {
        let len = bytes.len() as u64;
        let align = 4u64;
        let start = (self.head + (align - 1)) & !(align - 1);
        let end = start + len;
        if end > self.cap {
            self.head = 0;
            let start = 0;
            let end = len.min(self.cap);
            queue.write_buffer(&self.buf, start, &bytes[0..end as usize]);
            self.head = end;
            (start, len.min(self.cap - start))
        } else {
            queue.write_buffer(&self.buf, start, bytes);
            self.head = end;
            (start, len)
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    ndc_to_px: [f32; 2],
    _pad: [f32; 2],
}

pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    rect_pipeline: wgpu::RenderPipeline,
    border_pipeline: wgpu::RenderPipeline,
    ellipse_pipeline: wgpu::RenderPipeline,
    ellipse_border_pipeline: wgpu::RenderPipeline,
    text_pipeline_mask: wgpu::RenderPipeline,
    text_pipeline_color: wgpu::RenderPipeline,
    text_bind_layout: wgpu::BindGroupLayout,

    // Stencil clip pipelines
    clip_pipeline_a2c: wgpu::RenderPipeline,
    clip_pipeline_bin: wgpu::RenderPipeline,
    msaa_samples: u32,

    // Depth-stencil target
    depth_stencil_tex: wgpu::Texture,
    depth_stencil_view: wgpu::TextureView,

    // Optional MSAA color target
    msaa_tex: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,

    globals_layout: wgpu::BindGroupLayout,
    globals_buf: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,

    // Glyph atlas
    atlas_mask: AtlasA8,
    atlas_color: AtlasRGBA,

    // per-frame upload rings
    ring_rect: UploadRing,
    ring_border: UploadRing,
    ring_ellipse: UploadRing,
    ring_ellipse_border: UploadRing,
    ring_glyph_mask: UploadRing,
    ring_glyph_color: UploadRing,
    ring_clip: UploadRing,

    next_image_handle: u64,
    images: std::collections::HashMap<u64, ImageTex>,
}

struct ImageTex {
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    w: u32,
    h: u32,
}

struct AtlasA8 {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    next_x: u32,
    next_y: u32,
    row_h: u32,
    map: HashMap<(repose_text::GlyphKey, u32), GlyphInfo>,
}

struct AtlasRGBA {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    next_x: u32,
    next_y: u32,
    row_h: u32,
    map: HashMap<(repose_text::GlyphKey, u32), GlyphInfo>,
}

#[derive(Clone, Copy)]
struct GlyphInfo {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    w: f32,
    h: f32,
    bearing_x: f32,
    bearing_y: f32,
    advance: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectInstance {
    // xy in NDC, wh in NDC extents
    xywh: [f32; 4],
    // radius in NDC units
    radius: f32,
    brush_type: u32,
    color0: [f32; 4],
    color1: [f32; 4],
    grad_start: [f32; 2],
    grad_end: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BorderInstance {
    xywh: [f32; 4],
    radius: f32,
    stroke: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EllipseInstance {
    xywh: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EllipseBorderInstance {
    xywh: [f32; 4],
    stroke: f32,
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ClipInstance {
    xywh: [f32; 4],
    radius: f32,
    _pad: [f32; 3],
}

fn swash_to_a8_coverage(content: cosmic_text::SwashContent, data: &[u8]) -> Option<Vec<u8>> {
    match content {
        cosmic_text::SwashContent::Mask => Some(data.to_vec()),
        cosmic_text::SwashContent::SubpixelMask => {
            let mut out = Vec::with_capacity(data.len() / 4);
            for px in data.chunks_exact(4) {
                let r = px[0];
                let g = px[1];
                let b = px[2];
                out.push(r.max(g).max(b));
            }
            Some(out)
        }
        cosmic_text::SwashContent::Color => None,
    }
}

impl WgpuBackend {
    pub async fn new_async(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        let mut desc = wgpu::InstanceDescriptor::from_env_or_default();
        let instance: Instance;

        if cfg!(target_arch = "wasm32") {
            desc.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance = wgpu::util::new_instance_with_webgpu_detection(&desc).await;
        } else {
            instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        };

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("No suitable adapter: {e:?}"))?;

        let limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("repose-rs device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| anyhow::anyhow!("request_device failed: {e:?}"))?;

        let size = window.inner_size();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let present_mode = caps
            .present_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::PresentMode::Mailbox || *m == wgpu::PresentMode::Immediate)
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = caps.alpha_modes[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals buf"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });

        // Pick MSAA sample count
        let fmt_features = adapter.get_texture_format_features(format);
        let msaa_samples = if fmt_features.flags.sample_count_supported(4)
            && fmt_features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        {
            4
        } else {
            1
        };

        let ds_format = wgpu::TextureFormat::Depth24PlusStencil8;

        let stencil_for_content = wgpu::DepthStencilState {
            format: ds_format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                read_mask: 0xFF,
                write_mask: 0x00,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        let stencil_for_clip_inc = wgpu::DepthStencilState {
            format: ds_format,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::IncrementClamp,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Equal,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::IncrementClamp,
                },
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        let multisample_state = wgpu::MultisampleState {
            count: msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // Rect pipeline
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/rect.wgsl"))),
        });
        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect pipeline layout"),
            bind_group_layouts: &[&globals_layout],
            immediate_size: 0,
        });
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 2,
                            offset: 20,
                            format: wgpu::VertexFormat::Uint32,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 3,
                            offset: 24,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 4,
                            offset: 40,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 5,
                            offset: 56,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 6,
                            offset: 64,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: multisample_state,
            multiview_mask: None,
            cache: None,
        });

        // Border pipeline
        let border_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("border.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/border.wgsl"))),
        });
        let border_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("border pipeline layout"),
                bind_group_layouts: &[&globals_layout],
                immediate_size: 0,
            });
        let border_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("border pipeline"),
            layout: Some(&border_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &border_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<BorderInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 2,
                            offset: 20,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 3,
                            offset: 24,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &border_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: multisample_state,
            multiview_mask: None,
            cache: None,
        });

        // Ellipse pipeline
        let ellipse_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ellipse.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/ellipse.wgsl"))),
        });
        let ellipse_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ellipse pipeline layout"),
                bind_group_layouts: &[&globals_layout],
                immediate_size: 0,
            });
        let ellipse_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ellipse pipeline"),
            layout: Some(&ellipse_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ellipse_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<EllipseInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ellipse_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: multisample_state,
            multiview_mask: None,
            cache: None,
        });

        // Ellipse border pipeline
        let ellipse_border_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ellipse_border.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/ellipse_border.wgsl"
            ))),
        });
        let ellipse_border_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ellipse border layout"),
                bind_group_layouts: &[&globals_layout],
                immediate_size: 0,
            });
        let ellipse_border_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ellipse border pipeline"),
                layout: Some(&ellipse_border_layout),
                vertex: wgpu::VertexState {
                    module: &ellipse_border_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<EllipseBorderInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                shader_location: 0,
                                offset: 0,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                shader_location: 1,
                                offset: 16,
                                format: wgpu::VertexFormat::Float32,
                            },
                            wgpu::VertexAttribute {
                                shader_location: 2,
                                offset: 20,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                        ],
                    }],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &ellipse_border_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: config.format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(stencil_for_content.clone()),
                multisample: multisample_state,
                multiview_mask: None,
                cache: None,
            });

        // Text pipelines
        let text_mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/text.wgsl"))),
        });
        let text_color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_color.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/text_color.wgsl"
            ))),
        });
        let text_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text bind layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[&globals_layout, &text_bind_layout],
            immediate_size: 0,
        });
        let text_pipeline_mask = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline (mask)"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_mask_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 2,
                            offset: 32,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_mask_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: multisample_state,
            multiview_mask: None,
            cache: None,
        });
        let text_pipeline_color = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline (color)"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_color_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 2,
                            offset: 32,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_color_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: multisample_state,
            multiview_mask: None,
            cache: None,
        });

        // Clip pipelines
        let clip_shader_a2c = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clip_round_rect_a2c.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/clip_round_rect_a2c.wgsl"
            ))),
        });
        let clip_shader_bin = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clip_round_rect_bin.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/clip_round_rect_bin.wgsl"
            ))),
        });

        let clip_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clip pipeline layout"),
            bind_group_layouts: &[&globals_layout],
            immediate_size: 0,
        });

        let clip_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ClipInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    shader_location: 0,
                    offset: 0,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 1,
                    offset: 16,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let clip_color_target = wgpu::ColorTargetState {
            format: config.format,
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
        };

        let clip_pipeline_a2c = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (a2c)"),
            layout: Some(&clip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clip_shader_a2c,
                entry_point: Some("vs_main"),
                buffers: &[clip_vertex_layout.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &clip_shader_a2c,
                entry_point: Some("fs_main"),
                targets: &[Some(clip_color_target.clone())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_clip_inc.clone()),
            multisample: wgpu::MultisampleState {
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: msaa_samples > 1,
            },
            multiview_mask: None,
            cache: None,
        });

        let clip_pipeline_bin = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (bin)"),
            layout: Some(&clip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clip_shader_bin,
                entry_point: Some("vs_main"),
                buffers: &[clip_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &clip_shader_bin,
                entry_point: Some("fs_main"),
                targets: &[Some(clip_color_target)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_clip_inc),
            multisample: wgpu::MultisampleState {
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        // Atlases
        let atlas_mask = Self::init_atlas_mask(&device)?;
        let atlas_color = Self::init_atlas_color(&device)?;

        // Upload rings
        let ring_rect = UploadRing::new(&device, "ring rect", 1 << 20);
        let ring_border = UploadRing::new(&device, "ring border", 1 << 20);
        let ring_ellipse = UploadRing::new(&device, "ring ellipse", 1 << 20);
        let ring_ellipse_border = UploadRing::new(&device, "ring ellipse border", 1 << 20);
        let ring_glyph_mask = UploadRing::new(&device, "ring glyph mask", 1 << 20);
        let ring_glyph_color = UploadRing::new(&device, "ring glyph color", 1 << 20);
        let ring_clip = UploadRing::new(&device, "ring clip", 1 << 16);

        // Placeholder textures
        let depth_stencil_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("temp ds"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_stencil_view =
            depth_stencil_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let mut backend = Self {
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            border_pipeline,
            text_pipeline_mask,
            text_pipeline_color,
            text_bind_layout,
            ellipse_pipeline,
            ellipse_border_pipeline,
            atlas_mask,
            atlas_color,
            ring_rect,
            ring_border,
            ring_ellipse,
            ring_ellipse_border,
            ring_glyph_color,
            ring_glyph_mask,
            ring_clip,
            next_image_handle: 1,
            images: HashMap::new(),
            clip_pipeline_a2c,
            clip_pipeline_bin,
            msaa_samples,
            depth_stencil_tex,
            depth_stencil_view,
            msaa_tex: None,
            msaa_view: None,
            globals_bind,
            globals_buf,
            globals_layout,
        };

        backend.recreate_msaa_and_depth_stencil();
        Ok(backend)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async(window))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(_window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        anyhow::bail!("Use WgpuBackend::new_async(window).await on wasm32")
    }

    fn recreate_msaa_and_depth_stencil(&mut self) {
        if self.msaa_samples > 1 {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa color"),
                size: wgpu::Extent3d {
                    width: self.config.width.max(1),
                    height: self.config.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.msaa_samples,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.msaa_tex = Some(tex);
            self.msaa_view = Some(view);
        } else {
            self.msaa_tex = None;
            self.msaa_view = None;
        }

        self.depth_stencil_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-stencil (stencil clips)"),
            size: wgpu::Extent3d {
                width: self.config.width.max(1),
                height: self.config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.msaa_samples,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_stencil_view = self
            .depth_stencil_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
    }

    pub fn register_image_from_bytes(&mut self, data: &[u8], srgb: bool) -> u64 {
        let img = image::load_from_memory(data).expect("decode image");
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("user image"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfoBase {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image bind"),
            layout: &self.text_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_color.sampler),
                },
            ],
        });
        let handle = self.next_image_handle;
        self.next_image_handle += 1;
        self.images.insert(handle, ImageTex { view, bind, w, h });
        handle
    }

    fn init_atlas_mask(device: &wgpu::Device) -> anyhow::Result<AtlasA8> {
        let size = 1024u32;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas A8"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler A8"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        Ok(AtlasA8 {
            tex,
            view,
            sampler,
            size,
            next_x: 1,
            next_y: 1,
            row_h: 0,
            map: HashMap::new(),
        })
    }

    fn init_atlas_color(device: &wgpu::Device) -> anyhow::Result<AtlasRGBA> {
        let size = 1024u32;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas RGBA"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas sampler RGBA"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Ok(AtlasRGBA {
            tex,
            view,
            sampler,
            size,
            next_x: 1,
            next_y: 1,
            row_h: 0,
            map: HashMap::new(),
        })
    }

    fn atlas_bind_group_mask(&self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bind"),
            layout: &self.text_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_mask.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_mask.sampler),
                },
            ],
        })
    }

    fn atlas_bind_group_color(&self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bind color"),
            layout: &self.text_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.atlas_color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_color.sampler),
                },
            ],
        })
    }

    fn upload_glyph_mask(&mut self, key: repose_text::GlyphKey, px: u32) -> Option<GlyphInfo> {
        let keyp = (key, px);
        if let Some(info) = self.atlas_mask.map.get(&keyp) {
            return Some(*info);
        }

        let gb = repose_text::rasterize(key, px as f32)?;
        if gb.w == 0 || gb.h == 0 || gb.data.is_empty() {
            return None;
        }

        let coverage = match swash_to_a8_coverage(gb.content, &gb.data) {
            Some(c) => c,
            None => return None,
        };

        let w = gb.w.max(1);
        let h = gb.h.max(1);

        if !self.alloc_space_mask(w, h) {
            self.grow_mask_and_rebuild();
        }
        if !self.alloc_space_mask(w, h) {
            return None;
        }
        let x = self.atlas_mask.next_x;
        let y = self.atlas_mask.next_y;
        self.atlas_mask.next_x += w + 1;
        self.atlas_mask.row_h = self.atlas_mask.row_h.max(h + 1);

        let layout = wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w),
            rows_per_image: Some(h),
        };
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfoBase {
                texture: &self.atlas_mask.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &coverage,
            layout,
            size,
        );

        let info = GlyphInfo {
            u0: x as f32 / self.atlas_mask.size as f32,
            v0: y as f32 / self.atlas_mask.size as f32,
            u1: (x + w) as f32 / self.atlas_mask.size as f32,
            v1: (y + h) as f32 / self.atlas_mask.size as f32,
            w: w as f32,
            h: h as f32,
            bearing_x: 0.0,
            bearing_y: 0.0,
            advance: 0.0,
        };
        self.atlas_mask.map.insert(keyp, info);
        Some(info)
    }

    fn upload_glyph_color(&mut self, key: repose_text::GlyphKey, px: u32) -> Option<GlyphInfo> {
        let keyp = (key, px);
        if let Some(info) = self.atlas_color.map.get(&keyp) {
            return Some(*info);
        }
        let gb = repose_text::rasterize(key, px as f32)?;
        if !matches!(gb.content, cosmic_text::SwashContent::Color) {
            return None;
        }
        let w = gb.w.max(1);
        let h = gb.h.max(1);
        if !self.alloc_space_color(w, h) {
            self.grow_color_and_rebuild();
        }
        if !self.alloc_space_color(w, h) {
            return None;
        }
        let x = self.atlas_color.next_x;
        let y = self.atlas_color.next_y;
        self.atlas_color.next_x += w + 1;
        self.atlas_color.row_h = self.atlas_color.row_h.max(h + 1);

        let layout = wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        };
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfoBase {
                texture: &self.atlas_color.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &gb.data,
            layout,
            size,
        );
        let info = GlyphInfo {
            u0: x as f32 / self.atlas_color.size as f32,
            v0: y as f32 / self.atlas_color.size as f32,
            u1: (x + w) as f32 / self.atlas_color.size as f32,
            v1: (y + h) as f32 / self.atlas_color.size as f32,
            w: w as f32,
            h: h as f32,
            bearing_x: 0.0,
            bearing_y: 0.0,
            advance: 0.0,
        };
        self.atlas_color.map.insert(keyp, info);
        Some(info)
    }

    fn alloc_space_mask(&mut self, w: u32, h: u32) -> bool {
        if self.atlas_mask.next_x + w + 1 >= self.atlas_mask.size {
            self.atlas_mask.next_x = 1;
            self.atlas_mask.next_y += self.atlas_mask.row_h + 1;
            self.atlas_mask.row_h = 0;
        }
        if self.atlas_mask.next_y + h + 1 >= self.atlas_mask.size {
            return false;
        }
        true
    }

    fn grow_mask_and_rebuild(&mut self) {
        let new_size = (self.atlas_mask.size * 2).min(4096);
        if new_size == self.atlas_mask.size {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas A8 (grown)"),
            size: wgpu::Extent3d {
                width: new_size,
                height: new_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.atlas_mask.tex = tex;
        self.atlas_mask.view = self
            .atlas_mask
            .tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.atlas_mask.size = new_size;
        self.atlas_mask.next_x = 1;
        self.atlas_mask.next_y = 1;
        self.atlas_mask.row_h = 0;
        let keys: Vec<(repose_text::GlyphKey, u32)> = self.atlas_mask.map.keys().copied().collect();
        self.atlas_mask.map.clear();
        for (k, px) in keys {
            let _ = self.upload_glyph_mask(k, px);
        }
    }

    fn alloc_space_color(&mut self, w: u32, h: u32) -> bool {
        if self.atlas_color.next_x + w + 1 >= self.atlas_color.size {
            self.atlas_color.next_x = 1;
            self.atlas_color.next_y += self.atlas_color.row_h + 1;
            self.atlas_color.row_h = 0;
        }
        if self.atlas_color.next_y + h + 1 >= self.atlas_color.size {
            return false;
        }
        true
    }

    fn grow_color_and_rebuild(&mut self) {
        let new_size = (self.atlas_color.size * 2).min(4096);
        if new_size == self.atlas_color.size {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas RGBA (grown)"),
            size: wgpu::Extent3d {
                width: new_size,
                height: new_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.atlas_color.tex = tex;
        self.atlas_color.view = self
            .atlas_color
            .tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.atlas_color.size = new_size;
        self.atlas_color.next_x = 1;
        self.atlas_color.next_y = 1;
        self.atlas_color.row_h = 0;
        let keys: Vec<(repose_text::GlyphKey, u32)> =
            self.atlas_color.map.keys().copied().collect();
        self.atlas_color.map.clear();
        for (k, px) in keys {
            let _ = self.upload_glyph_color(k, px);
        }
    }
}

fn brush_to_instance_fields(brush: &Brush) -> (u32, [f32; 4], [f32; 4], [f32; 2], [f32; 2]) {
    match brush {
        Brush::Solid(c) => (
            0u32,
            c.to_linear(),
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
        ),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => (
            1u32,
            start_color.to_linear(),
            end_color.to_linear(),
            [start.x, start.y],
            [end.x, end.y],
        ),
    }
}

fn brush_to_solid_color(brush: &Brush) -> [f32; 4] {
    match brush {
        Brush::Solid(c) => c.to_linear(),
        Brush::Linear { start_color, .. } => start_color.to_linear(),
    }
}

impl RenderBackend for WgpuBackend {
    fn configure_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.recreate_msaa_and_depth_stencil();
    }

    fn frame(&mut self, scene: &Scene, _glyph_cfg: GlyphRasterConfig) {
        if self.config.width == 0 || self.config.height == 0 {
            return;
        }
        let frame = loop {
            match self.surface.get_current_texture() {
                Ok(f) => break f,
                Err(wgpu::SurfaceError::Lost) => {
                    log::warn!("surface lost; reconfiguring");
                    self.surface.configure(&self.device, &self.config);
                }
                Err(wgpu::SurfaceError::Outdated) => {
                    log::warn!("surface outdated; reconfiguring");
                    self.surface.configure(&self.device, &self.config);
                }
                Err(wgpu::SurfaceError::Timeout) => {
                    log::warn!("surface timeout; retrying");
                    continue;
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    log::error!("surface OOM");
                    return;
                }
                Err(wgpu::SurfaceError::Other) => {
                    log::error!("Other error");
                    return;
                }
            }
        };

        fn to_ndc(x: f32, y: f32, w: f32, h: f32, fb_w: f32, fb_h: f32) -> [f32; 4] {
            let x0 = (x / fb_w) * 2.0 - 1.0;
            let y0 = 1.0 - (y / fb_h) * 2.0;
            let x1 = ((x + w) / fb_w) * 2.0 - 1.0;
            let y1 = 1.0 - ((y + h) / fb_h) * 2.0;
            let min_x = x0.min(x1);
            let min_y = y0.min(y1);
            let w_ndc = (x1 - x0).abs();
            let h_ndc = (y1 - y0).abs();
            [min_x, min_y, w_ndc, h_ndc]
        }

        fn to_ndc_scalar(px: f32, fb_dim: f32) -> f32 {
            (px / fb_dim) * 2.0
        }

        fn to_scissor(r: &repose_core::Rect, fb_w: u32, fb_h: u32) -> (u32, u32, u32, u32) {
            let mut x = r.x.floor() as i64;
            let mut y = r.y.floor() as i64;
            let fb_wi = fb_w as i64;
            let fb_hi = fb_h as i64;
            x = x.clamp(0, fb_wi.saturating_sub(1));
            y = y.clamp(0, fb_hi.saturating_sub(1));
            let w_req = r.w.ceil().max(1.0) as i64;
            let h_req = r.h.ceil().max(1.0) as i64;
            let w = (w_req).min(fb_wi - x).max(1);
            let h = (h_req).min(fb_hi - y).max(1);
            (x as u32, y as u32, w as u32, h as u32)
        }

        let fb_w = self.config.width as f32;
        let fb_h = self.config.height as f32;

        let globals = Globals {
            ndc_to_px: [fb_w * 0.5, fb_h * 0.5],
            _pad: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));

        enum Cmd {
            ClipPush {
                off: u64,
                cnt: u32,
                scissor: (u32, u32, u32, u32),
            },
            ClipPop {
                scissor: (u32, u32, u32, u32),
            },
            Rect {
                off: u64,
                cnt: u32,
            },
            Border {
                off: u64,
                cnt: u32,
            },
            Ellipse {
                off: u64,
                cnt: u32,
            },
            EllipseBorder {
                off: u64,
                cnt: u32,
            },
            GlyphsMask {
                off: u64,
                cnt: u32,
            },
            GlyphsColor {
                off: u64,
                cnt: u32,
            },
            Image {
                off: u64,
                cnt: u32,
                handle: u64,
            },
            PushTransform(Transform),
            PopTransform,
        }

        let mut cmds: Vec<Cmd> = Vec::with_capacity(scene.nodes.len());

        struct Batch {
            rects: Vec<RectInstance>,
            borders: Vec<BorderInstance>,
            ellipses: Vec<EllipseInstance>,
            e_borders: Vec<EllipseBorderInstance>,
            masks: Vec<GlyphInstance>,
            colors: Vec<GlyphInstance>,
        }

        impl Batch {
            fn new() -> Self {
                Self {
                    rects: vec![],
                    borders: vec![],
                    ellipses: vec![],
                    e_borders: vec![],
                    masks: vec![],
                    colors: vec![],
                }
            }

            fn flush(
                &mut self,
                rings: (
                    &mut UploadRing,
                    &mut UploadRing,
                    &mut UploadRing,
                    &mut UploadRing,
                    &mut UploadRing,
                    &mut UploadRing,
                ),
                device: &wgpu::Device,
                queue: &wgpu::Queue,
                cmds: &mut Vec<Cmd>,
            ) {
                let (
                    ring_rect,
                    ring_border,
                    ring_ellipse,
                    ring_ellipse_border,
                    ring_mask,
                    ring_color,
                ) = rings;

                if !self.rects.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.rects);
                    ring_rect.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_rect.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::Rect {
                        off,
                        cnt: self.rects.len() as u32,
                    });
                    self.rects.clear();
                }
                if !self.borders.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.borders);
                    ring_border.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_border.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::Border {
                        off,
                        cnt: self.borders.len() as u32,
                    });
                    self.borders.clear();
                }
                if !self.ellipses.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.ellipses);
                    ring_ellipse.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_ellipse.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::Ellipse {
                        off,
                        cnt: self.ellipses.len() as u32,
                    });
                    self.ellipses.clear();
                }
                if !self.e_borders.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.e_borders);
                    ring_ellipse_border.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_ellipse_border.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::EllipseBorder {
                        off,
                        cnt: self.e_borders.len() as u32,
                    });
                    self.e_borders.clear();
                }
                if !self.masks.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.masks);
                    ring_mask.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_mask.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::GlyphsMask {
                        off,
                        cnt: self.masks.len() as u32,
                    });
                    self.masks.clear();
                }
                if !self.colors.is_empty() {
                    let bytes = bytemuck::cast_slice(&self.colors);
                    ring_color.grow_to_fit(device, bytes.len() as u64);
                    let (off, wrote) = ring_color.alloc_write(queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    cmds.push(Cmd::GlyphsColor {
                        off,
                        cnt: self.colors.len() as u32,
                    });
                    self.colors.clear();
                }
            }
        }

        self.ring_rect.reset();
        self.ring_border.reset();
        self.ring_ellipse.reset();
        self.ring_ellipse_border.reset();
        self.ring_glyph_mask.reset();
        self.ring_glyph_color.reset();
        self.ring_clip.reset();

        let mut batch = Batch::new();
        let mut transform_stack: Vec<Transform> = vec![Transform::identity()];
        let mut scissor_stack: Vec<repose_core::Rect> = Vec::with_capacity(8);
        let root_clip_rect = repose_core::Rect {
            x: 0.0,
            y: 0.0,
            w: fb_w,
            h: fb_h,
        };

        for node in &scene.nodes {
            let t_identity = Transform::identity();
            let current_transform = transform_stack.last().unwrap_or(&t_identity);

            match node {
                SceneNode::Rect {
                    rect,
                    brush,
                    radius,
                } => {
                    let transformed_rect = current_transform.apply_to_rect(*rect);
                    let (brush_type, color0, color1, grad_start, grad_end) =
                        brush_to_instance_fields(brush);
                    batch.rects.push(RectInstance {
                        xywh: to_ndc(
                            transformed_rect.x,
                            transformed_rect.y,
                            transformed_rect.w,
                            transformed_rect.h,
                            fb_w,
                            fb_h,
                        ),
                        radius: *radius,
                        brush_type,
                        color0,
                        color1,
                        grad_start,
                        grad_end,
                    });
                }
                SceneNode::Border {
                    rect,
                    color,
                    width,
                    radius,
                } => {
                    let transformed_rect = current_transform.apply_to_rect(*rect);
                    batch.borders.push(BorderInstance {
                        xywh: to_ndc(
                            transformed_rect.x,
                            transformed_rect.y,
                            transformed_rect.w,
                            transformed_rect.h,
                            fb_w,
                            fb_h,
                        ),
                        radius: *radius,
                        stroke: *width,
                        color: color.to_linear(),
                    });
                }
                SceneNode::Ellipse { rect, brush } => {
                    let transformed = current_transform.apply_to_rect(*rect);
                    let color = brush_to_solid_color(brush);
                    batch.ellipses.push(EllipseInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            fb_w,
                            fb_h,
                        ),
                        color,
                    });
                }
                SceneNode::EllipseBorder { rect, color, width } => {
                    let transformed = current_transform.apply_to_rect(*rect);
                    batch.e_borders.push(EllipseBorderInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            fb_w,
                            fb_h,
                        ),
                        stroke: *width,
                        color: color.to_linear(),
                    });
                }
                SceneNode::Text {
                    rect,
                    text,
                    color,
                    size,
                } => {
                    let px = (*size).clamp(8.0, 96.0);
                    let shaped = repose_text::shape_line(text.as_ref(), px);
                    let transformed_rect = current_transform.apply_to_rect(*rect);

                    for sg in shaped {
                        if let Some(info) = self.upload_glyph_color(sg.key, px as u32) {
                            let x = transformed_rect.x + sg.x + sg.bearing_x;
                            let y = transformed_rect.y + sg.y - sg.bearing_y;
                            batch.colors.push(GlyphInstance {
                                xywh: to_ndc(x, y, info.w, info.h, fb_w, fb_h),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: [1.0, 1.0, 1.0, 1.0],
                            });
                        } else if let Some(info) = self.upload_glyph_mask(sg.key, px as u32) {
                            let x = transformed_rect.x + sg.x + sg.bearing_x;
                            let y = transformed_rect.y + sg.y - sg.bearing_y;
                            batch.masks.push(GlyphInstance {
                                xywh: to_ndc(x, y, info.w, info.h, fb_w, fb_h),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                            });
                        }
                    }
                }
                SceneNode::Image {
                    rect,
                    handle,
                    tint,
                    fit,
                } => {
                    let tex = if let Some(t) = self.images.get(handle) {
                        t
                    } else {
                        log::warn!("Image handle {} not found", handle);
                        continue;
                    };
                    let src_w = tex.w as f32;
                    let src_h = tex.h as f32;
                    let dst_w = rect.w.max(0.0);
                    let dst_h = rect.h.max(0.0);
                    if dst_w <= 0.0 || dst_h <= 0.0 {
                        continue;
                    }
                    let (xywh_ndc, uv_rect) = match fit {
                        repose_core::view::ImageFit::Contain => {
                            let scale = (dst_w / src_w).min(dst_h / src_h);
                            let w = src_w * scale;
                            let h = src_h * scale;
                            let x = rect.x + (dst_w - w) * 0.5;
                            let y = rect.y + (dst_h - h) * 0.5;
                            (to_ndc(x, y, w, h, fb_w, fb_h), [0.0, 1.0, 1.0, 0.0])
                        }
                        repose_core::view::ImageFit::Cover => {
                            let scale = (dst_w / src_w).max(dst_h / src_h);
                            let content_w = src_w * scale;
                            let content_h = src_h * scale;
                            let overflow_x = (content_w - dst_w) * 0.5;
                            let overflow_y = (content_h - dst_h) * 0.5;
                            let u0 = (overflow_x / content_w).clamp(0.0, 1.0);
                            let v0 = (overflow_y / content_h).clamp(0.0, 1.0);
                            let u1 = ((overflow_x + dst_w) / content_w).clamp(0.0, 1.0);
                            let v1 = ((overflow_y + dst_h) / content_h).clamp(0.0, 1.0);
                            (
                                to_ndc(rect.x, rect.y, dst_w, dst_h, fb_w, fb_h),
                                [u0, 1.0 - v1, u1, 1.0 - v0],
                            )
                        }
                        repose_core::view::ImageFit::FitWidth => {
                            let scale = dst_w / src_w;
                            let w = dst_w;
                            let h = src_h * scale;
                            let y = rect.y + (dst_h - h) * 0.5;
                            (to_ndc(rect.x, y, w, h, fb_w, fb_h), [0.0, 1.0, 1.0, 0.0])
                        }
                        repose_core::view::ImageFit::FitHeight => {
                            let scale = dst_h / src_h;
                            let w = src_w * scale;
                            let h = dst_h;
                            let x = rect.x + (dst_w - w) * 0.5;
                            (to_ndc(x, rect.y, w, h, fb_w, fb_h), [0.0, 1.0, 1.0, 0.0])
                        }
                    };
                    let inst = GlyphInstance {
                        xywh: xywh_ndc,
                        uv: uv_rect,
                        color: tint.to_linear(),
                    };
                    let bytes = bytemuck::bytes_of(&inst);
                    self.ring_glyph_color
                        .grow_to_fit(&self.device, bytes.len() as u64);
                    let (off, wrote) = self.ring_glyph_color.alloc_write(&self.queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());
                    batch.flush(
                        (
                            &mut self.ring_rect,
                            &mut self.ring_border,
                            &mut self.ring_ellipse,
                            &mut self.ring_ellipse_border,
                            &mut self.ring_glyph_mask,
                            &mut self.ring_glyph_color,
                        ),
                        &self.device,
                        &self.queue,
                        &mut cmds,
                    );
                    cmds.push(Cmd::Image {
                        off,
                        cnt: 1,
                        handle: *handle,
                    });
                }
                SceneNode::PushClip { rect, radius } => {
                    batch.flush(
                        (
                            &mut self.ring_rect,
                            &mut self.ring_border,
                            &mut self.ring_ellipse,
                            &mut self.ring_ellipse_border,
                            &mut self.ring_glyph_mask,
                            &mut self.ring_glyph_color,
                        ),
                        &self.device,
                        &self.queue,
                        &mut cmds,
                    );

                    let t_identity = Transform::identity();
                    let current_transform = transform_stack.last().unwrap_or(&t_identity);
                    let transformed = current_transform.apply_to_rect(*rect);

                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let next_scissor = intersect(top, transformed);
                    scissor_stack.push(next_scissor);
                    let scissor = to_scissor(&next_scissor, self.config.width, self.config.height);

                    let inst = ClipInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            fb_w,
                            fb_h,
                        ),
                        radius: *radius,
                        _pad: [0.0; 3],
                    };
                    let bytes = bytemuck::bytes_of(&inst);
                    self.ring_clip.grow_to_fit(&self.device, bytes.len() as u64);
                    let (off, wrote) = self.ring_clip.alloc_write(&self.queue, bytes);
                    debug_assert_eq!(wrote as usize, bytes.len());

                    cmds.push(Cmd::ClipPush {
                        off,
                        cnt: 1,
                        scissor,
                    });
                }
                SceneNode::PopClip => {
                    batch.flush(
                        (
                            &mut self.ring_rect,
                            &mut self.ring_border,
                            &mut self.ring_ellipse,
                            &mut self.ring_ellipse_border,
                            &mut self.ring_glyph_mask,
                            &mut self.ring_glyph_color,
                        ),
                        &self.device,
                        &self.queue,
                        &mut cmds,
                    );

                    if !scissor_stack.is_empty() {
                        scissor_stack.pop();
                    } else {
                        log::warn!("PopClip with empty stack");
                    }

                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let scissor = to_scissor(&top, self.config.width, self.config.height);
                    cmds.push(Cmd::ClipPop { scissor });
                }
                SceneNode::PushTransform { transform } => {
                    let combined = current_transform.combine(transform);
                    if transform.rotate != 0.0 {
                        ROT_WARN_ONCE.call_once(|| {
                            log::warn!(
                                "Transform rotation is not supported for Rect/Text/Image; rotation will be ignored."
                            );
                        });
                    }
                    transform_stack.push(combined);
                }
                SceneNode::PopTransform => {
                    transform_stack.pop();
                }
            }
        }

        batch.flush(
            (
                &mut self.ring_rect,
                &mut self.ring_border,
                &mut self.ring_ellipse,
                &mut self.ring_ellipse_border,
                &mut self.ring_glyph_mask,
                &mut self.ring_glyph_color,
            ),
            &self.device,
            &self.queue,
            &mut cmds,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let swap_view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let (color_view, resolve_target) = if let Some(msaa_view) = &self.msaa_view {
                (msaa_view, Some(&swap_view))
            } else {
                (&swap_view, None)
            };

            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: scene.clear_color.0 as f64 / 255.0,
                            g: scene.clear_color.1 as f64 / 255.0,
                            b: scene.clear_color.2 as f64 / 255.0,
                            a: scene.clear_color.3 as f64 / 255.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_stencil_view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut clip_depth: u32 = 0;
            rpass.set_bind_group(0, &self.globals_bind, &[]);
            rpass.set_stencil_reference(clip_depth);
            rpass.set_scissor_rect(0, 0, self.config.width, self.config.height);

            let bind_mask = self.atlas_bind_group_mask();
            let bind_color = self.atlas_bind_group_color();

            for cmd in cmds {
                match cmd {
                    Cmd::ClipPush {
                        off,
                        cnt: n,
                        scissor,
                    } => {
                        rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                        rpass.set_stencil_reference(clip_depth);

                        if self.msaa_samples > 1 {
                            rpass.set_pipeline(&self.clip_pipeline_a2c);
                        } else {
                            rpass.set_pipeline(&self.clip_pipeline_bin);
                        }

                        let bytes = (n as u64) * std::mem::size_of::<ClipInstance>() as u64;
                        rpass.set_vertex_buffer(0, self.ring_clip.buf.slice(off..off + bytes));
                        rpass.draw(0..6, 0..n);

                        clip_depth = (clip_depth + 1).min(255);
                        rpass.set_stencil_reference(clip_depth);
                    }

                    Cmd::ClipPop { scissor } => {
                        clip_depth = clip_depth.saturating_sub(1);
                        rpass.set_stencil_reference(clip_depth);
                        rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                    }

                    Cmd::Rect { off, cnt: n } => {
                        rpass.set_pipeline(&self.rect_pipeline);
                        let bytes = (n as u64) * std::mem::size_of::<RectInstance>() as u64;
                        rpass.set_vertex_buffer(0, self.ring_rect.buf.slice(off..off + bytes));
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::Border { off, cnt: n } => {
                        rpass.set_pipeline(&self.border_pipeline);
                        let bytes = (n as u64) * std::mem::size_of::<BorderInstance>() as u64;
                        rpass.set_vertex_buffer(0, self.ring_border.buf.slice(off..off + bytes));
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::GlyphsMask { off, cnt: n } => {
                        rpass.set_pipeline(&self.text_pipeline_mask);
                        rpass.set_bind_group(1, &bind_mask, &[]);
                        let bytes = (n as u64) * std::mem::size_of::<GlyphInstance>() as u64;
                        rpass
                            .set_vertex_buffer(0, self.ring_glyph_mask.buf.slice(off..off + bytes));
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::GlyphsColor { off, cnt: n } => {
                        rpass.set_pipeline(&self.text_pipeline_color);
                        rpass.set_bind_group(1, &bind_color, &[]);
                        let bytes = (n as u64) * std::mem::size_of::<GlyphInstance>() as u64;
                        rpass.set_vertex_buffer(
                            0,
                            self.ring_glyph_color.buf.slice(off..off + bytes),
                        );
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::Image {
                        off,
                        cnt: n,
                        handle,
                    } => {
                        if let Some(tex) = self.images.get(&handle) {
                            rpass.set_pipeline(&self.text_pipeline_color);
                            rpass.set_bind_group(1, &tex.bind, &[]);
                            let bytes = (n as u64) * std::mem::size_of::<GlyphInstance>() as u64;
                            rpass.set_vertex_buffer(
                                0,
                                self.ring_glyph_color.buf.slice(off..off + bytes),
                            );
                            rpass.draw(0..6, 0..n);
                        } else {
                            log::warn!("Image handle {} not found; skipping draw", handle);
                        }
                    }

                    Cmd::Ellipse { off, cnt: n } => {
                        rpass.set_pipeline(&self.ellipse_pipeline);
                        let bytes = (n as u64) * std::mem::size_of::<EllipseInstance>() as u64;
                        rpass.set_vertex_buffer(0, self.ring_ellipse.buf.slice(off..off + bytes));
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::EllipseBorder { off, cnt: n } => {
                        rpass.set_pipeline(&self.ellipse_border_pipeline);
                        let bytes =
                            (n as u64) * std::mem::size_of::<EllipseBorderInstance>() as u64;
                        rpass.set_vertex_buffer(
                            0,
                            self.ring_ellipse_border.buf.slice(off..off + bytes),
                        );
                        rpass.draw(0..6, 0..n);
                    }

                    Cmd::PushTransform(_) => {}
                    Cmd::PopTransform => {}
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| frame.present())) {
            log::warn!("frame.present panicked: {:?}", e);
        }
    }
}

fn intersect(a: repose_core::Rect, b: repose_core::Rect) -> repose_core::Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    repose_core::Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}
