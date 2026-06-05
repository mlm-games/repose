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
        let start = (self.head + 3) & !3;
        if start + needed <= self.cap {
            return;
        }
        let new_cap = (start + needed).next_power_of_two();
        self.buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("upload ring (grown)"),
            size: new_cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.cap = new_cap;
    }

    fn alloc_write(&mut self, queue: &wgpu::Queue, bytes: &[u8]) -> (u64, u64) {
        let len = bytes.len() as u64;
        let start = (self.head + 3) & !3; // align to 4
        let end = start + len;
        assert!(end <= self.cap, "ring overflow - call grow_to_fit first");
        queue.write_buffer(&self.buf, start, bytes);
        self.head = end;
        (start, len)
    }
}

struct InstancedPipe<I: bytemuck::Pod> {
    ring: UploadRing,
    stride: u64,
    _marker: std::marker::PhantomData<I>,
}

impl<I: bytemuck::Pod> InstancedPipe<I> {
    fn new(ring: UploadRing) -> Self {
        Self {
            ring,
            stride: std::mem::size_of::<I>() as u64,
            _marker: std::marker::PhantomData,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[I],
    ) -> Option<(u64, u32)> {
        if data.is_empty() {
            return None;
        }
        let bytes = bytemuck::cast_slice(data);
        self.ring.grow_to_fit(device, bytes.len() as u64);
        let (off, wrote) = self.ring.alloc_write(queue, bytes);
        debug_assert_eq!(wrote as usize, bytes.len());
        Some((off, data.len() as u32))
    }

    fn reset(&mut self) {
        self.ring.reset();
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

    // Render pipelines. Two sets: one for the MSAA surface pass, one for
    // graphics-layer render-to-texture passes (sample_count = 1).
    surface_pipes: Pipelines,
    layer_pipes: Pipelines,

    // Instanced draw rings
    rects: InstancedPipe<RectInstance>,
    borders: InstancedPipe<BorderInstance>,
    ellipses: InstancedPipe<EllipseInstance>,
    ellipse_borders: InstancedPipe<EllipseBorderInstance>,
    glyph_mask: InstancedPipe<GlyphInstance>,
    glyph_color: InstancedPipe<GlyphInstance>,

    // Image bind layouts and shared sampler
    image_bind_layout_rgba: wgpu::BindGroupLayout,
    image_bind_layout_nv12: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,

    // Blur composite ring (for graphics-layer drop shadows)
    blur_ring: UploadRing,

    text_bind_layout: wgpu::BindGroupLayout,

    // Stencil clip ring
    clip_ring: UploadRing,

    // Instanced NV12 ring
    nv12: InstancedPipe<Nv12Instance>,

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

    // Image management
    next_image_handle: u64,
    images: HashMap<u64, ImageTex>,

    // Eviction stats
    frame_index: u64,
    image_bytes_total: u64,
    image_evict_after_frames: u64,
    image_budget_bytes: u64,

    // Graphics layer pool. Maps `SceneNode::BeginLayer::layer_id` to a
    // cached offscreen render target.
    layer_pool: HashMap<u32, LayerTarget>,
}

impl Drop for WgpuBackend {
    fn drop(&mut self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
}

#[derive(Clone)]
struct LayerTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    depth_stencil_tex: wgpu::Texture,
    depth_stencil_view: wgpu::TextureView,
    width: u32,
    height: u32,
    rect_px: (f32, f32, f32, f32),
}

/// Identifies which render target a `Pass` draws into.
#[derive(Clone, Copy)]
enum PassTarget {
    Surface,
    Layer(u32),
}

/// A bundle of render pipelines for a single sample-count target. Created
/// twice: once with `sample_count = msaa_samples` for the surface pass, and
/// once with `sample_count = 1` for graphics-layer render-to-texture passes
/// (where MSAA is wasted).
struct Pipelines {
    rects: wgpu::RenderPipeline,
    borders: wgpu::RenderPipeline,
    ellipses: wgpu::RenderPipeline,
    ellipse_borders: wgpu::RenderPipeline,
    text_mask: wgpu::RenderPipeline,
    text_color: wgpu::RenderPipeline,
    image_rgba: wgpu::RenderPipeline,
    image_nv12: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    clip_a2c: wgpu::RenderPipeline,
    clip_bin: wgpu::RenderPipeline,
}

impl Pipelines {
    fn create(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        sample_count: u32,
        globals_layout: &wgpu::BindGroupLayout,
        text_bind_layout: &wgpu::BindGroupLayout,
        image_bind_layout_nv12: &wgpu::BindGroupLayout,
        clip_pipeline_layout: &wgpu::PipelineLayout,
        stencil_for_content: &wgpu::DepthStencilState,
        stencil_for_clip_inc: &wgpu::DepthStencilState,
        clip_color_target: &wgpu::ColorTargetState,
        clip_vertex_layout: &wgpu::VertexBufferLayout,
    ) -> Self {
        let msaa_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        macro_rules! make_content_pipeline {
            ($name:ident, $shader:literal, $inst_type:ty, $attrs:expr) => {
                let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(concat!($shader, ".wgsl")),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(concat!("shaders/", $shader, ".wgsl")))),
                });
                let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(concat!($shader, " pipeline layout")),
                    bind_group_layouts: &[Some(globals_layout)],
                    immediate_size: 0,
                });
                let $name = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(concat!($shader, " pipeline")),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader_module,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<$inst_type>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: $attrs,
                        }],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader_module,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Some(stencil_for_content.clone()),
                    multisample: msaa_state,
                    multiview_mask: None,
                    cache: None,
                });
            };
        }

        let rect_attrs: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 1, offset: 16, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { shader_location: 2, offset: 20, format: wgpu::VertexFormat::Uint32 },
            wgpu::VertexAttribute { shader_location: 3, offset: 24, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 4, offset: 40, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 5, offset: 56, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { shader_location: 6, offset: 64, format: wgpu::VertexFormat::Float32x2 },
        ];
        let border_attrs: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 1, offset: 16, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { shader_location: 2, offset: 20, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { shader_location: 3, offset: 24, format: wgpu::VertexFormat::Float32x4 },
        ];
        let ellipse_attrs: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 1, offset: 16, format: wgpu::VertexFormat::Float32x4 },
        ];
        let ellipse_border_attrs: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x4 },
            wgpu::VertexAttribute { shader_location: 1, offset: 16, format: wgpu::VertexFormat::Float32 },
            wgpu::VertexAttribute { shader_location: 2, offset: 20, format: wgpu::VertexFormat::Float32x4 },
        ];

        make_content_pipeline!(rects, "rect", RectInstance, rect_attrs);
        make_content_pipeline!(borders, "border", BorderInstance, border_attrs);
        make_content_pipeline!(ellipses, "ellipse", EllipseInstance, ellipse_attrs);
        make_content_pipeline!(ellipse_borders, "ellipse_border", EllipseBorderInstance, ellipse_border_attrs);

        // Text (mask)
        let text_mask_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/text.wgsl"))),
        });
        // Text (color)
        let text_color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_color.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/text_color.wgsl"
            ))),
        });
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(globals_layout), Some(text_bind_layout)],
            immediate_size: 0,
        });
        let glyph_vertex = wgpu::VertexBufferLayout {
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
        };
        let text_mask = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline (mask)"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_mask_shader,
                entry_point: Some("vs_main"),
                buffers: &[glyph_vertex.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_mask_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: msaa_state,
            multiview_mask: None,
            cache: None,
        });
        let text_color = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline (color)"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_color_shader,
                entry_point: Some("vs_main"),
                buffers: &[glyph_vertex],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_color_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: msaa_state,
            multiview_mask: None,
            cache: None,
        });
        // image_rgba reuses the text color pipeline (same vertex/bindings).
        let image_rgba = text_color.clone();

        // Blur composite pipeline (graphics-layer drop shadow)
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_shadow.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/blur_shadow.wgsl"
            ))),
        });
        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur pipeline layout"),
            bind_group_layouts: &[Some(globals_layout), Some(text_bind_layout)],
            immediate_size: 0,
        });
        let blur = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<BlurInstance>() as u64,
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
                        wgpu::VertexAttribute {
                            shader_location: 3,
                            offset: 48,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: msaa_state,
            multiview_mask: None,
            cache: None,
        });

        // NV12 Image Pipeline
        let image_nv12_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image_nv12.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/image_nv12.wgsl"
            ))),
        });
        let image_nv12_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image nv12 pipeline layout"),
            bind_group_layouts: &[Some(globals_layout), Some(image_bind_layout_nv12)],
            immediate_size: 0,
        });
        let image_nv12 = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image nv12 pipeline"),
            layout: Some(&image_nv12_layout),
            vertex: wgpu::VertexState {
                module: &image_nv12_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Nv12Instance>() as u64,
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
                        wgpu::VertexAttribute {
                            shader_location: 3,
                            offset: 48,
                            format: wgpu::VertexFormat::Float32,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_nv12_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_content.clone()),
            multisample: msaa_state,
            multiview_mask: None,
            cache: None,
        });

        // Clipping
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
        let clip_a2c = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (a2c)"),
            layout: Some(clip_pipeline_layout),
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
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: sample_count > 1,
            },
            multiview_mask: None,
            cache: None,
        });
        let clip_bin = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (bin)"),
            layout: Some(clip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clip_shader_bin,
                entry_point: Some("vs_main"),
                buffers: &[clip_vertex_layout.clone()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &clip_shader_bin,
                entry_point: Some("fs_main"),
                targets: &[Some(clip_color_target.clone())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_clip_inc.clone()),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            rects,
            borders,
            ellipses,
            ellipse_borders,
            text_mask,
            text_color,
            image_rgba,
            image_nv12,
            blur,
            clip_a2c,
            clip_bin,
        }
    }
}

/// A segment of the frame that draws into a single render target.
struct Pass {
    target: PassTarget,
    /// The initial scissor to apply to the rpass when it is opened.
    initial_scissor: (u32, u32, u32, u32),
    /// `None` means `LoadOp::Load` (resume existing content);
    /// `Some(c)` means `LoadOp::Clear(c)`.
    clear_color: Option<[f32; 4]>,
    cmds: Vec<Cmd>,
}

#[allow(non_snake_case)]
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
    ImageRgba {
        off: u64,
        cnt: u32,
        handle: u64,
    },
    ImageNv12 {
        off: u64,
        cnt: u32,
        handle: u64,
    },
    PushTransform(Transform),
    PopTransform,
    /// Composite a previously-rendered graphics layer back into the
    /// current target as a textured quad. The quad's vertex buffer
    /// lives in `self.glyph_color.ring` (a `GlyphInstance`).
    CompositeLayer {
        off: u64,
        cnt: u32,
        layer_id: u32,
        alpha: f32,
    },
    /// Composite a blurred drop shadow of a previously-rendered graphics
    /// layer. The quad's vertex buffer lives in `self.blur_ring` (a
    /// `BlurInstance`).
    CompositeShadow {
        off: u64,
        cnt: u32,
        layer_id: u32,
    },
}

enum ImageTex {
    Rgba {
        tex: wgpu::Texture,
        view: wgpu::TextureView,
        bind: wgpu::BindGroup,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
        last_used_frame: u64,
        bytes: u64,
    },
    Nv12 {
        tex_y: wgpu::Texture,
        view_y: wgpu::TextureView,
        tex_uv: wgpu::Texture,
        view_uv: wgpu::TextureView,
        bind: wgpu::BindGroup,
        w: u32,
        h: u32,
        full_range: bool,
        last_used_frame: u64,
        bytes: u64,
    },
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
    xywh: [f32; 4],
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
struct BlurInstance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    blur_uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Nv12Instance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4], // tint
    full_range: f32,
    _pad: [f32; 3],
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
        let instance: Instance;

        if cfg!(target_arch = "wasm32") {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
        } else {
            instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
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
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::LessEqual,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::Keep,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::LessEqual,
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
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
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

        let _multisample_state = wgpu::MultisampleState {
            count: msaa_samples,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // PIPELINES

        // Single shared sampler for images/text
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image/text sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // Layout for Text / RGBA Images (Texture + Sampler)
        let text_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text/rgba bind layout"),
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
        // We reuse this for RGBA images for simplicity, or create a distinct one
        let image_bind_layout_rgba = text_bind_layout.clone();

        // Layout for NV12 Images (TextureY + TextureUV + Sampler)
        let image_bind_layout_nv12 =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image bind layout nv12"),
                entries: &[
                    // Y plane
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
                    // UV plane
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // Sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Clipping layout
        let clip_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("clip pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout)],
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

        // Two sets of pipelines: one for the MSAA surface pass, one for layer
        // render-to-texture passes (sample_count = 1).
        let surface_pipes = Pipelines::create(
            &device,
            config.format,
            msaa_samples,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &clip_color_target,
            &clip_vertex_layout,
        );
        let layer_pipes = Pipelines::create(
            &device,
            config.format,
            1,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &clip_color_target,
            &clip_vertex_layout,
        );

        // Blur composite ring (for graphics-layer drop shadows)
        let blur_ring = UploadRing::new(&device, "blur ring", 1024 * 1024);

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
        let ring_nv12 = UploadRing::new(&device, "ring nv12", 1 << 20);

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

            surface_pipes,
            layer_pipes,

            rects: InstancedPipe::new(ring_rect),
            borders: InstancedPipe::new(ring_border),
            ellipses: InstancedPipe::new(ring_ellipse),
            ellipse_borders: InstancedPipe::new(ring_ellipse_border),
            glyph_mask: InstancedPipe::new(ring_glyph_mask),
            glyph_color: InstancedPipe::new(ring_glyph_color),

            text_bind_layout,

            image_bind_layout_rgba,
            image_bind_layout_nv12,
            image_sampler,

            blur_ring,

            clip_ring: ring_clip,

            nv12: InstancedPipe::new(ring_nv12),

            msaa_samples,
            depth_stencil_tex,
            depth_stencil_view,
            msaa_tex: None,
            msaa_view: None,
            globals_bind,
            globals_buf,
            globals_layout,

            atlas_mask,
            atlas_color,

            next_image_handle: 1,
            images: HashMap::new(),

            frame_index: 0,
            image_bytes_total: 0,
            image_evict_after_frames: 600,         // ~10s @ 60fps
            image_budget_bytes: 512 * 1024 * 1024, // 512 MB
            layer_pool: HashMap::new(),
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

    // Image API

    pub fn set_image_from_bytes(
        &mut self,
        handle: u64,
        data: &[u8],
        srgb: bool,
    ) -> anyhow::Result<()> {
        let img = image::load_from_memory(data)?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        self.set_image_rgba8(handle, w, h, &rgba, srgb)
    }

    pub fn set_image_rgba8(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        rgba: &[u8],
        srgb: bool,
    ) -> anyhow::Result<()> {
        let expected = (w as usize) * (h as usize) * 4;
        if rgba.len() < expected {
            return Err(anyhow::anyhow!(
                "RGBA buffer too small: {} < {}",
                rgba.len(),
                expected
            ));
        }

        let format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let needs_recreate = match self.images.get(&handle) {
            Some(ImageTex::Rgba {
                w: cw,
                h: ch,
                format: cf,
                ..
            }) => *cw != w || *ch != h || *cf != format,
            _ => true,
        };

        if needs_recreate {
            // Remove old to track budget correctly
            self.remove_image(handle);

            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("user image rgba"),
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
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("image bind rgba"),
                layout: &self.image_bind_layout_rgba,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                    },
                ],
            });

            let bytes = (w as u64) * (h as u64) * 4;
            self.image_bytes_total += bytes;

            self.images.insert(
                handle,
                ImageTex::Rgba {
                    tex,
                    view,
                    bind,
                    w,
                    h,
                    format,
                    last_used_frame: self.frame_index,
                    bytes,
                },
            );
        }

        let tex = match self.images.get(&handle) {
            Some(ImageTex::Rgba { tex, .. }) => tex,
            _ => unreachable!(),
        };

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba[..expected],
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

        // Ensure budget limits
        self.evict_budget_excess();

        Ok(())
    }

    pub fn set_image_nv12(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        y: &[u8],
        uv: &[u8],
        full_range: bool,
    ) -> anyhow::Result<()> {
        let y_expected = (w as usize) * (h as usize);
        let uv_w = (w / 2).max(1);
        let uv_h = (h / 2).max(1);
        let uv_expected = (uv_w as usize) * (uv_h as usize) * 2;

        if y.len() < y_expected {
            return Err(anyhow::anyhow!("Y plane too small"));
        }
        if uv.len() < uv_expected {
            return Err(anyhow::anyhow!("UV plane too small"));
        }

        let needs_recreate = match self.images.get(&handle) {
            Some(ImageTex::Nv12 { w: ww, h: hh, .. }) => *ww != w || *hh != h,
            _ => true,
        };

        if needs_recreate {
            self.remove_image(handle);

            let tex_y = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("nv12 Y"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view_y = tex_y.create_view(&wgpu::TextureViewDescriptor::default());

            let tex_uv = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("nv12 UV"),
                size: wgpu::Extent3d {
                    width: uv_w,
                    height: uv_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view_uv = tex_uv.create_view(&wgpu::TextureViewDescriptor::default());

            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nv12 bind"),
                layout: &self.image_bind_layout_nv12,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view_y),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view_uv),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                    },
                ],
            });

            let bytes = (w as u64) * (h as u64) + (uv_w as u64) * (uv_h as u64) * 2;
            self.image_bytes_total += bytes;

            self.images.insert(
                handle,
                ImageTex::Nv12 {
                    tex_y,
                    view_y,
                    tex_uv,
                    view_uv,
                    bind,
                    w,
                    h,
                    full_range,
                    last_used_frame: self.frame_index,
                    bytes,
                },
            );
        }

        let (tex_y, tex_uv, _bind) = match self.images.get(&handle) {
            Some(ImageTex::Nv12 {
                tex_y,
                tex_uv,
                bind,
                ..
            }) => (tex_y, tex_uv, bind),
            _ => return Err(anyhow::anyhow!("Handle is not NV12")),
        };

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex_y,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &y[..y_expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: tex_uv,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &uv[..uv_expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(2 * uv_w),
                rows_per_image: Some(uv_h),
            },
            wgpu::Extent3d {
                width: uv_w,
                height: uv_h,
                depth_or_array_layers: 1,
            },
        );

        self.evict_budget_excess();
        Ok(())
    }

    pub fn remove_image(&mut self, handle: u64) {
        if let Some(img) = self.images.remove(&handle) {
            let b = match img {
                ImageTex::Rgba { bytes, .. } => bytes,
                ImageTex::Nv12 { bytes, .. } => bytes,
            };
            self.image_bytes_total = self.image_bytes_total.saturating_sub(b);
        }
    }

    // Legacy support from Step 1 instructions (temporary until platform render logic is fully swapped)
    pub fn register_image_from_bytes(&mut self, data: &[u8], srgb: bool) -> u64 {
        let handle = self.next_image_handle;
        self.next_image_handle += 1;
        if let Err(e) = self.set_image_from_bytes(handle, data, srgb) {
            log::error!("Failed to register image: {e}");
        }
        handle
    }

    fn evict_unused_images(&mut self) {
        let now = self.frame_index;
        let evict_after = self.image_evict_after_frames;

        // Time based eviction
        let mut to_remove = Vec::new();
        for (h, t) in self.images.iter() {
            let last = match t {
                ImageTex::Rgba {
                    last_used_frame, ..
                } => *last_used_frame,
                ImageTex::Nv12 {
                    last_used_frame, ..
                } => *last_used_frame,
            };
            if now.saturating_sub(last) > evict_after {
                to_remove.push(*h);
            }
        }
        for h in to_remove {
            self.remove_image(h);
        }

        self.evict_budget_excess();
    }

    fn evict_budget_excess(&mut self) {
        if self.image_bytes_total <= self.image_budget_bytes {
            return;
        }
        // Collect (handle, last_used, bytes)
        let mut candidates: Vec<(u64, u64, u64)> = self
            .images
            .iter()
            .map(|(h, t)| {
                let (last, bytes) = match t {
                    ImageTex::Rgba {
                        last_used_frame,
                        bytes,
                        ..
                    } => (*last_used_frame, *bytes),
                    ImageTex::Nv12 {
                        last_used_frame,
                        bytes,
                        ..
                    } => (*last_used_frame, *bytes),
                };
                (*h, last, bytes)
            })
            .collect();

        // Sort by last_used ascending (LRU first)
        candidates.sort_by_key(|k| k.1);

        let now = self.frame_index;
        for (h, last, _bytes) in candidates {
            if self.image_bytes_total <= self.image_budget_bytes {
                break;
            }
            // Don't evict something used this frame
            if last == now {
                continue;
            }
            self.remove_image(h);
        }
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

    fn get_or_create_layer(
        &mut self,
        layer_id: u32,
        width: u32,
        height: u32,
        rect: repose_core::Rect,
    ) {
        let needs_alloc = match self.layer_pool.get(&layer_id) {
            Some(lt) => lt.width != width || lt.height != height,
            None => true,
        };
        if !needs_alloc {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("graphics layer"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("layer bind"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
            ],
        });
        let depth_stencil_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("graphics layer depth-stencil"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
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
        self.layer_pool.insert(
            layer_id,
            LayerTarget {
                texture: tex,
                view,
                bind,
                depth_stencil_tex,
                depth_stencil_view,
                width,
                height,
                rect_px: (rect.x, rect.y, rect.w, rect.h),
            },
        );
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

        let coverage = swash_to_a8_coverage(gb.content, &gb.data)?;

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
        _ => (0u32, [0.0; 4], [0.0; 4], [0.0; 2], [0.0; 2]),
    }
}

fn brush_to_solid_color(brush: &Brush) -> [f32; 4] {
    match brush {
        Brush::Solid(c) => c.to_linear(),
        Brush::Linear { start_color, .. } => start_color.to_linear(),
        _ => [0.0; 4],
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
        // Frame start maintenance
        self.frame_index = self.frame_index.wrapping_add(1);

        if self.config.width == 0 || self.config.height == 0 {
            return;
        }
        let mut retries = 0u32;
        const MAX_RETRIES: u32 = 4;
        let frame = loop {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) => break f,
                wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                    log::warn!("suboptimal surface; reconfiguring");
                    self.surface.configure(&self.device, &self.config);
                    break f;
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    log::warn!("surface outdated; reconfiguring");
                    self.surface.configure(&self.device, &self.config);
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    log::warn!("surface lost; reconfiguring");
                    self.surface.configure(&self.device, &self.config);
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return;
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        log::warn!(
                            "surface validation persisted after {MAX_RETRIES} retries; skipping frame"
                        );
                        return;
                    }
                    self.surface.configure(&self.device, &self.config);
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

        let mut passes: Vec<Pass> = Vec::with_capacity(1);
        let mut current_pass: Pass = Pass {
            target: PassTarget::Surface,
            initial_scissor: (0, 0, self.config.width, self.config.height),
            clear_color: Some([
                scene.clear_color.0 as f32 / 255.0,
                scene.clear_color.1 as f32 / 255.0,
                scene.clear_color.2 as f32 / 255.0,
                scene.clear_color.3 as f32 / 255.0,
            ]),
            cmds: Vec::with_capacity(scene.nodes.len()),
        };
        let mut target_stack: Vec<PassTarget> = Vec::new();
        let mut layer_alphas: Vec<(u32, f32, (u32, u32, u32, u32))> = Vec::new();
        let mut current_target_size: (f32, f32) = (fb_w, fb_h);

        struct Batch {
            rects: Vec<RectInstance>,
            borders: Vec<BorderInstance>,
            ellipses: Vec<EllipseInstance>,
            e_borders: Vec<EllipseBorderInstance>,
            masks: Vec<GlyphInstance>,
            colors: Vec<GlyphInstance>,
            nv12s: Vec<Nv12Instance>,
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
                    nv12s: vec![],
                }
            }

            fn is_empty(&self) -> bool {
                self.rects.is_empty()
                    && self.borders.is_empty()
                    && self.ellipses.is_empty()
                    && self.e_borders.is_empty()
                    && self.masks.is_empty()
                    && self.colors.is_empty()
                    && self.nv12s.is_empty()
            }

            fn flush(
                &mut self,
                pipes: (
                    &mut InstancedPipe<RectInstance>,
                    &mut InstancedPipe<BorderInstance>,
                    &mut InstancedPipe<EllipseInstance>,
                    &mut InstancedPipe<EllipseBorderInstance>,
                ),
                glyph_pipes: (
                    &mut InstancedPipe<GlyphInstance>,
                    &mut InstancedPipe<GlyphInstance>,
                ),
                nv12_pipe: &mut InstancedPipe<Nv12Instance>,
                device: &wgpu::Device,
                queue: &wgpu::Queue,
                cmds: &mut Vec<Cmd>,
            ) {
                let (rects, borders, ellipses, e_borders) = pipes;
                let (masks, colors) = glyph_pipes;

                macro_rules! flush_one {
                    ($buf:ident, $pipe:expr, $variant:ident) => {
                        if !self.$buf.is_empty() {
                            if let Some((off, cnt)) = $pipe.upload(device, queue, &self.$buf) {
                                cmds.push(Cmd::$variant { off, cnt });
                            }
                            self.$buf.clear();
                        }
                    };
                }

                flush_one!(rects, rects, Rect);
                flush_one!(borders, borders, Border);
                flush_one!(ellipses, ellipses, Ellipse);
                flush_one!(e_borders, e_borders, EllipseBorder);
                flush_one!(masks, masks, GlyphsMask);
                flush_one!(colors, colors, GlyphsColor);

                if !self.nv12s.is_empty() {
                    if let Some((off, cnt)) = nv12_pipe.upload(device, queue, &self.nv12s) {
                        let _ = (off, cnt);
                    }
                    self.nv12s.clear();
                }
            }
        }

        self.rects.reset();
        self.borders.reset();
        self.ellipses.reset();
        self.ellipse_borders.reset();
        self.glyph_mask.reset();
        self.glyph_color.reset();
        self.clip_ring.reset();
        self.blur_ring.reset();
        self.nv12.reset();

        let mut batch = Batch::new();
        let mut transform_stack: Vec<Transform> = vec![Transform::identity()];
        let mut scissor_stack: Vec<repose_core::Rect> = Vec::with_capacity(8);
        let root_clip_rect = repose_core::Rect {
            x: 0.0,
            y: 0.0,
            w: fb_w,
            h: fb_h,
        };

        let mut current_prim: Option<&'static str> = None;

        macro_rules! flush_if_prim_changed {
            ($prim:literal, $pipe:expr) => {
                if current_prim != Some($prim) {
                    flush_batch!();
                    current_prim = Some($prim);
                }
            };
        }

        macro_rules! flush_batch {
            () => {
                if !batch.is_empty() {
                    batch.flush(
                        (
                            &mut self.rects,
                            &mut self.borders,
                            &mut self.ellipses,
                            &mut self.ellipse_borders,
                        ),
                        (&mut self.glyph_mask, &mut self.glyph_color),
                        &mut self.nv12,
                        &self.device,
                        &self.queue,
                        &mut current_pass.cmds,
                    )
                }
                current_prim = None;
            };
        }

        for node in &scene.nodes {
            let t_identity = Transform::identity();
            let current_transform = transform_stack.last().unwrap_or(&t_identity);

            match node {
                SceneNode::Rect {
                    rect,
                    brush,
                    radius,
                } => {
                    flush_if_prim_changed!("rect", &self.rects);
                    let transformed_rect = current_transform.apply_to_rect(*rect);
                    let (brush_type, color0, color1, grad_start, grad_end) =
                        brush_to_instance_fields(brush);
                    batch.rects.push(RectInstance {
                        xywh: to_ndc(
                            transformed_rect.x,
                            transformed_rect.y,
                            transformed_rect.w,
                            transformed_rect.h,
                            current_target_size.0,
                            current_target_size.1,
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
                    flush_if_prim_changed!("border", &self.borders);
                    let transformed_rect = current_transform.apply_to_rect(*rect);
                    batch.borders.push(BorderInstance {
                        xywh: to_ndc(
                            transformed_rect.x,
                            transformed_rect.y,
                            transformed_rect.w,
                            transformed_rect.h,
                            current_target_size.0,
                            current_target_size.1,
                        ),
                        radius: *radius,
                        stroke: *width,
                        color: color.to_linear(),
                    });
                }
                SceneNode::Ellipse { rect, brush } => {
                    flush_if_prim_changed!("ellipse", &self.ellipses);
                    let transformed = current_transform.apply_to_rect(*rect);
                    let color = brush_to_solid_color(brush);
                    batch.ellipses.push(EllipseInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            current_target_size.0,
                            current_target_size.1,
                        ),
                        color,
                    });
                }
                SceneNode::EllipseBorder { rect, color, width } => {
                    flush_if_prim_changed!("ellipse_border", &self.ellipse_borders);
                    let transformed = current_transform.apply_to_rect(*rect);
                    batch.e_borders.push(EllipseBorderInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            current_target_size.0,
                            current_target_size.1,
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
                    font_family,
                } => {
                    flush_batch!(); // flush any prior primitives

                    let px = (*size).clamp(8.0, 96.0);
                    let shaped = repose_text::shape_line(text.as_ref(), px, *font_family);
                    let transformed_rect = current_transform.apply_to_rect(*rect);

                    for sg in shaped {
                        if let Some(info) = self.upload_glyph_color(sg.key, px as u32) {
                            let x = transformed_rect.x + sg.x + sg.bearing_x;
                            let y = transformed_rect.y + sg.y - sg.bearing_y;
                            batch.colors.push(GlyphInstance {
                                xywh: to_ndc(
                                    x,
                                    y,
                                    info.w,
                                    info.h,
                                    current_target_size.0,
                                    current_target_size.1,
                                ),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                            });
                        } else if let Some(info) = self.upload_glyph_mask(sg.key, px as u32) {
                            let x = transformed_rect.x + sg.x + sg.bearing_x;
                            let y = transformed_rect.y + sg.y - sg.bearing_y;
                            batch.masks.push(GlyphInstance {
                                xywh: to_ndc(
                                    x,
                                    y,
                                    info.w,
                                    info.h,
                                    current_target_size.0,
                                    current_target_size.1,
                                ),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                            });
                        }
                    }
                    // Don't flush here - let next primitive trigger flush
                }
                SceneNode::Image {
                    rect,
                    handle,
                    tint,
                    fit,
                } => {
                    flush_batch!();

                    // Update usage timestamp for eviction
                    let (img_w, img_h, is_nv12) = if let Some(t) = self.images.get_mut(handle) {
                        match t {
                            ImageTex::Rgba {
                                w,
                                h,
                                last_used_frame,
                                ..
                            } => {
                                *last_used_frame = self.frame_index;
                                (*w, *h, false)
                            }
                            ImageTex::Nv12 {
                                w,
                                h,
                                last_used_frame,
                                ..
                            } => {
                                *last_used_frame = self.frame_index;
                                (*w, *h, true)
                            }
                        }
                    } else {
                        log::warn!("Image handle {} not found", handle);
                        continue;
                    };

                    let src_w = img_w as f32;
                    let src_h = img_h as f32;
                    let transformed = current_transform.apply_to_rect(*rect);
                    let dst_w = transformed.w.max(0.0);
                    let dst_h = transformed.h.max(0.0);
                    if dst_w <= 0.0 || dst_h <= 0.0 {
                        continue;
                    }

                    let (xywh_ndc, uv_rect) = match fit {
                        repose_core::view::ImageFit::Contain => {
                            let scale = (dst_w / src_w).min(dst_h / src_h);
                            let w = src_w * scale;
                            let h = src_h * scale;
                            let x = transformed.x + (dst_w - w) * 0.5;
                            let y = transformed.y + (dst_h - h) * 0.5;
                            (
                                to_ndc(x, y, w, h, current_target_size.0, current_target_size.1),
                                [0.0, 1.0, 1.0, 0.0],
                            )
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
                                to_ndc(
                                    transformed.x,
                                    transformed.y,
                                    dst_w,
                                    dst_h,
                                    current_target_size.0,
                                    current_target_size.1,
                                ),
                                [u0, 1.0 - v1, u1, 1.0 - v0],
                            )
                        }
                        repose_core::view::ImageFit::FitWidth => {
                            let scale = dst_w / src_w;
                            let w = dst_w;
                            let h = src_h * scale;
                            let y = transformed.y + (dst_h - h) * 0.5;
                            (
                                to_ndc(
                                    transformed.x,
                                    y,
                                    w,
                                    h,
                                    current_target_size.0,
                                    current_target_size.1,
                                ),
                                [0.0, 1.0, 1.0, 0.0],
                            )
                        }
                        repose_core::view::ImageFit::FitHeight => {
                            let scale = dst_h / src_h;
                            let w = src_w * scale;
                            let h = dst_h;
                            let x = transformed.x + (dst_w - w) * 0.5;
                            (
                                to_ndc(
                                    x,
                                    transformed.y,
                                    w,
                                    h,
                                    current_target_size.0,
                                    current_target_size.1,
                                ),
                                [0.0, 1.0, 1.0, 0.0],
                            )
                        }
                        _ => ([0.0; 4], [0.0; 4]),
                    };
 
                    if is_nv12 {
                        let full_range = if let Some(ImageTex::Nv12 { full_range, .. }) =
                            self.images.get(handle)
                        {
                            if *full_range { 1.0 } else { 0.0 }
                        } else {
                            0.0
                        };

                        let inst = Nv12Instance {
                            xywh: xywh_ndc,
                            uv: uv_rect,
                            color: tint.to_linear(),
                            full_range,
                            _pad: [0.0; 3],
                        };
                        if let Some((off, _)) = self.nv12.upload(&self.device, &self.queue, &[inst])
                        {
                            current_pass.cmds.push(Cmd::ImageNv12 {
                                off,
                                cnt: 1,
                                handle: *handle,
                            });
                        }
                    } else {
                        // RGBA uses GlyphInstance struct (reused pipeline)
                        let inst = GlyphInstance {
                            xywh: xywh_ndc,
                            uv: uv_rect,
                            color: tint.to_linear(),
                        };
                        if let Some((off, _)) =
                            self.glyph_color.upload(&self.device, &self.queue, &[inst])
                        {
                            current_pass.cmds.push(Cmd::ImageRgba {
                                off,
                                cnt: 1,
                                handle: *handle,
                            });
                        }
                    }
                }
                SceneNode::PushClip { rect, radius } => {
                    flush_batch!(); // flush content before entering clip

                    let t_identity = Transform::identity();
                    let current_transform = transform_stack.last().unwrap_or(&t_identity);
                    let transformed = current_transform.apply_to_rect(*rect);

                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let next_scissor = intersect(top, transformed);
                    scissor_stack.push(next_scissor);
                    let scissor = to_scissor(
                        &next_scissor,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );

                    let inst = ClipInstance {
                        xywh: to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            current_target_size.0,
                            current_target_size.1,
                        ),
                        radius: *radius,
                        _pad: [0.0; 3],
                    };
                    let bytes = bytemuck::bytes_of(&inst);
                    self.clip_ring.grow_to_fit(&self.device, bytes.len() as u64);
                    let (off, _) = self.clip_ring.alloc_write(&self.queue, bytes);

                    current_pass.cmds.push(Cmd::ClipPush {
                        off,
                        cnt: 1,
                        scissor,
                    });
                }
                SceneNode::PopClip => {
                    flush_batch!();

                    if !scissor_stack.is_empty() {
                        scissor_stack.pop();
                    } else {
                        log::warn!("PopClip with empty stack");
                    }

                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let scissor = to_scissor(
                        &top,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    current_pass.cmds.push(Cmd::ClipPop { scissor });
                }
                SceneNode::Shadow {
                    rect,
                    radius,
                    elevation: _,
                    color,
                } => {
                    flush_if_prim_changed!("rect", &self.rects);
                    let transformed_rect = current_transform.apply_to_rect(*rect);
                    let (brush_type, color0, _color1, _grad_start, _grad_end) =
                        brush_to_instance_fields(&Brush::Solid(*color));
                    batch.rects.push(RectInstance {
                        xywh: to_ndc(
                            transformed_rect.x,
                            transformed_rect.y,
                            transformed_rect.w,
                            transformed_rect.h,
                            current_target_size.0,
                            current_target_size.1,
                        ),
                        radius: *radius,
                        brush_type,
                        color0,
                        color1: [0.0; 4],
                        grad_start: [0.0; 2],
                        grad_end: [0.0; 2],
                    });
                }
                SceneNode::PushTransform { transform } => {
                    flush_batch!(); // flush before transform change
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
                    flush_batch!(); // flush before transform change
                    transform_stack.pop();
                }
                SceneNode::BeginLayer {
                    rect,
                    layer_id,
                    alpha,
                } => {
                    flush_batch!();
                    let w = (rect.w.max(1.0)).ceil() as u32;
                    let h = (rect.h.max(1.0)).ceil() as u32;
                    // Close out the current pass, start a new one for the layer.
                    let prev_target = current_pass.target;
                    let prev_scissor = current_pass.initial_scissor;
                    let saved = std::mem::replace(
                        &mut current_pass,
                        Pass {
                            target: PassTarget::Layer(*layer_id),
                            initial_scissor: (0, 0, w, h),
                            clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                            cmds: Vec::new(),
                        },
                    );
                    passes.push(saved);
                    target_stack.push(prev_target);
                    let _ = prev_scissor; // initial_scissor of resumed pass is restored at EndLayer
                    // Get or create the layer's offscreen texture now so that
                    // subsequent scissor ops / draws have a valid target.
                    self.get_or_create_layer(*layer_id, w, h, *rect);
                    current_target_size = (w as f32, h as f32);
                    layer_alphas.push((*layer_id, *alpha, current_pass.initial_scissor));
                }
                SceneNode::EndLayer { layer_id } => {
                    flush_batch!();
                    // Finish the layer's pass, start a new one on the previous target.
                    let saved = std::mem::replace(
                        &mut current_pass,
                        Pass {
                            target: target_stack.pop().unwrap_or(PassTarget::Surface),
                            initial_scissor: (0, 0, self.config.width, self.config.height),
                            clear_color: None, // LoadOp::Load - don't wipe earlier surface content
                            cmds: Vec::new(),
                        },
                    );
                    passes.push(saved);
                    current_target_size = (fb_w, fb_h);
                    // Issue a composite quad for the just-finished layer in the new pass.
                    if let Some((_, layer_alpha, _)) = layer_alphas
                        .iter()
                        .find(|(id, _, _)| id == layer_id)
                        .copied()
                    {
                        let layer = self.layer_pool.get(layer_id).expect("layer target");
                        let inst = GlyphInstance {
                            xywh: to_ndc(
                                layer.rect_px.0,
                                layer.rect_px.1,
                                layer.rect_px.2,
                                layer.rect_px.3,
                                fb_w,
                                fb_h,
                            ),
                            uv: [0.0, 1.0, 1.0, 0.0],
                            color: [1.0, 1.0, 1.0, layer_alpha],
                        };
                        if let Some((off, cnt)) =
                            self.glyph_color.upload(&self.device, &self.queue, &[inst])
                        {
                            current_pass.cmds.push(Cmd::CompositeLayer {
                                off,
                                cnt,
                                layer_id: *layer_id,
                                alpha: layer_alpha,
                            });
                        }
                    }
                }
                SceneNode::CompositeShadow {
                    layer_id,
                    blur_px,
                    offset_px,
                    color,
                } => {
                    flush_batch!();
                    if let Some(layer) = self.layer_pool.get(layer_id).cloned() {
                        // Shadow rect = layer rect + offset.
                        let sx = layer.rect_px.0 + offset_px.0;
                        let sy = layer.rect_px.1 + offset_px.1;
                        let sw = layer.rect_px.2;
                        let sh = layer.rect_px.3;
                        // The blur in UV space is 1.5 * blur_px / texture_size
                        // (the 1.5 matches the 3x3 Gaussian span).
                        let bw_uv = (blur_px * 1.5) / layer.width.max(1) as f32;
                        let bh_uv = (blur_px * 1.5) / layer.height.max(1) as f32;
                        let inst = BlurInstance {
                            xywh: to_ndc(sx, sy, sw, sh, fb_w, fb_h),
                            uv: [0.0, 0.0, 1.0, 1.0],
                            color: [
                                color.0 as f32 / 255.0,
                                color.1 as f32 / 255.0,
                                color.2 as f32 / 255.0,
                                color.3 as f32 / 255.0,
                            ],
                            blur_uv: [bw_uv, bh_uv],
                        };
                        self.blur_ring
                            .grow_to_fit(&self.device, std::mem::size_of::<BlurInstance>() as u64);
                        let bytes = bytemuck::bytes_of(&inst);
                        let (off, _) = self.blur_ring.alloc_write(&self.queue, bytes);
                        current_pass.cmds.push(Cmd::CompositeShadow {
                            off,
                            cnt: 1,
                            layer_id: *layer_id,
                        });
                    }
                }
                _ => {}
            }
        }

        flush_batch!();

        // Push the final pass.
        passes.push(current_pass);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        let bind_mask = self.atlas_bind_group_mask();
        let bind_color = self.atlas_bind_group_color();
        let mut clip_depth: u32 = 0;

        for pass in std::mem::take(&mut passes) {
            let (color_view, resolve_target, depth_stencil_view, is_layer) = match pass.target {
                PassTarget::Surface => {
                    let swap_view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let (color, resolve) = if let Some(msaa_view) = &self.msaa_view {
                        (msaa_view.clone(), Some(swap_view))
                    } else {
                        (swap_view, None)
                    };
                    (color, resolve, self.depth_stencil_view.clone(), false)
                }
                PassTarget::Layer(layer_id) => {
                    if let Some(lt) = self.layer_pool.get(&layer_id) {
                        (lt.view.clone(), None, lt.depth_stencil_view.clone(), true)
                    } else {
                        log::warn!("missing layer target {layer_id}");
                        continue;
                    }
                }
            };

            if is_layer {
                clip_depth = 0;
            }

            let pipes: &Pipelines = if is_layer {
                &self.layer_pipes
            } else {
                &self.surface_pipes
            };

            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    resolve_target: resolve_target.as_ref(),
                    ops: wgpu::Operations {
                        load: match pass.clear_color {
                            Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                                r: c[0] as f64,
                                g: c[1] as f64,
                                b: c[2] as f64,
                                a: c[3] as f64,
                            }),
                            None => wgpu::LoadOp::Load,
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_stencil_view,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: if is_layer || pass.clear_color.is_some() {
                            wgpu::LoadOp::Clear(0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            rpass.set_bind_group(0, &self.globals_bind, &[]);
            rpass.set_stencil_reference(clip_depth);
            rpass.set_scissor_rect(
                pass.initial_scissor.0,
                pass.initial_scissor.1,
                pass.initial_scissor.2,
                pass.initial_scissor.3,
            );

            macro_rules! draw_simple {
                ($pipeline:expr, $ring:expr, $inst:ty, $off:ident, $n:ident) => {{
                    rpass.set_pipeline($pipeline);
                    let bytes = ($n as u64) * std::mem::size_of::<$inst>() as u64;
                    rpass.set_vertex_buffer(0, $ring.buf.slice($off..$off + bytes));
                    rpass.draw(0..6, 0..$n);
                }};
            }

            macro_rules! draw_with_bind {
                ($pipeline:expr, $ring:expr, $inst:ty, $bind:expr, $off:ident, $n:ident) => {{
                    rpass.set_pipeline($pipeline);
                    rpass.set_bind_group(1, $bind, &[]);
                    let bytes = ($n as u64) * std::mem::size_of::<$inst>() as u64;
                    rpass.set_vertex_buffer(0, $ring.buf.slice($off..$off + bytes));
                    rpass.draw(0..6, 0..$n);
                }};
            }

            for cmd in pass.cmds {
                match cmd {
                    Cmd::ClipPush {
                        off,
                        cnt: n,
                        scissor,
                    } => {
                        rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                        rpass.set_stencil_reference(clip_depth);

                        if self.msaa_samples > 1 && !is_layer {
                            rpass.set_pipeline(&pipes.clip_a2c);
                        } else {
                            rpass.set_pipeline(&pipes.clip_bin);
                        }

                        let bytes = (n as u64) * std::mem::size_of::<ClipInstance>() as u64;
                        rpass.set_vertex_buffer(0, self.clip_ring.buf.slice(off..off + bytes));
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
                        draw_simple!(&pipes.rects, self.rects.ring, RectInstance, off, n);
                    }

                    Cmd::Border { off, cnt: n } => {
                        draw_simple!(&pipes.borders, self.borders.ring, BorderInstance, off, n);
                    }

                    Cmd::GlyphsMask { off, cnt: n } => {
                        draw_with_bind!(
                            &pipes.text_mask,
                            self.glyph_mask.ring,
                            GlyphInstance,
                            &bind_mask,
                            off,
                            n
                        );
                    }

                    Cmd::GlyphsColor { off, cnt: n } => {
                        draw_with_bind!(
                            &pipes.text_color,
                            self.glyph_color.ring,
                            GlyphInstance,
                            &bind_color,
                            off,
                            n
                        );
                    }

                    Cmd::ImageRgba {
                        off,
                        cnt: n,
                        handle,
                    } => {
                        if let Some(ImageTex::Rgba { bind, .. }) = self.images.get(&handle) {
                            draw_with_bind!(
                                &pipes.image_rgba,
                                self.glyph_color.ring,
                                GlyphInstance,
                                bind,
                                off,
                                n
                            );
                        }
                    }

                    Cmd::ImageNv12 {
                        off,
                        cnt: n,
                        handle,
                    } => {
                        if let Some(ImageTex::Nv12 { bind, .. }) = self.images.get(&handle) {
                            draw_with_bind!(
                                &pipes.image_nv12,
                                self.nv12.ring,
                                Nv12Instance,
                                bind,
                                off,
                                n
                            );
                        }
                    }

                    Cmd::Ellipse { off, cnt: n } => {
                        draw_simple!(&pipes.ellipses, self.ellipses.ring, EllipseInstance, off, n);
                    }

                    Cmd::EllipseBorder { off, cnt: n } => {
                        draw_simple!(
                            &pipes.ellipse_borders,
                            self.ellipse_borders.ring,
                            EllipseBorderInstance,
                            off,
                            n
                        );
                    }

                    Cmd::PushTransform(_) => {}
                    Cmd::PopTransform => {}
                    Cmd::CompositeLayer {
                        off,
                        cnt: n,
                        layer_id,
                        alpha: _,
                    } => {
                        if let Some(lt) = self.layer_pool.get(&layer_id).cloned() {
                            draw_with_bind!(
                                &pipes.image_rgba,
                                self.glyph_color.ring,
                                GlyphInstance,
                                &lt.bind,
                                off,
                                n
                            );
                        }
                    }
                    Cmd::CompositeShadow {
                        off,
                        cnt: n,
                        layer_id,
                    } => {
                        if let Some(lt) = self.layer_pool.get(&layer_id).cloned() {
                            draw_with_bind!(
                                &pipes.blur,
                                self.blur_ring,
                                BlurInstance,
                                &lt.bind,
                                off,
                                n
                            );
                        }
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| frame.present())) {
            log::warn!("frame.present panicked: {:?}", e);
        }

        // Frame end maintenance: Evict unused images
        self.evict_unused_images();
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
