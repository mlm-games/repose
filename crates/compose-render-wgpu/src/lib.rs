use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use ab_glyph::{point, Font, FontArc, Glyph, PxScale, ScaleFont};
use compose_core::{Color, GlyphRasterConfig, RenderBackend, Scene, SceneNode};
use cosmic_text;
use fontdb::Database;
use std::panic::{catch_unwind, AssertUnwindSafe};
use wgpu::util::DeviceExt;

pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    rect_pipeline: wgpu::RenderPipeline,
    // rect_bind_layout: wgpu::BindGroupLayout,
    border_pipeline: wgpu::RenderPipeline,
    // border_bind_layout: wgpu::BindGroupLayout,
    text_pipeline_mask: wgpu::RenderPipeline,
    text_pipeline_color: wgpu::RenderPipeline,
    text_bind_layout: wgpu::BindGroupLayout,

    // Glyph atlas
    atlas_mask: AtlasA8,
    atlas_color: AtlasRGBA,
}

struct AtlasA8 {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    next_x: u32,
    next_y: u32,
    row_h: u32,
    map: HashMap<(compose_text::GlyphKey, u32), GlyphInfo>,
}

struct AtlasRGBA {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    next_x: u32,
    next_y: u32,
    row_h: u32,
    map: HashMap<(compose_text::GlyphKey, u32), GlyphInfo>,
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
    // rgba (linear)
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BorderInstance {
    // outer rect in NDC
    xywh: [f32; 4],
    // outer radius in NDC
    radius_outer: f32,
    // stroke width in NDC
    stroke: f32,
    // rgba (linear)
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    // xywh in NDC
    xywh: [f32; 4],
    // uv
    uv: [f32; 4],
    // color
    color: [f32; 4],
}

impl WgpuBackend {
    pub fn new(window: Arc<winit::window::Window>) -> anyhow::Result<Self> {
        // Instance/Surface (latest API with backend options from env)
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        let surface = instance.create_surface(window.clone())?;

        // Adapter/Device
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|_e| anyhow::anyhow!("No adapter"))?;

        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("compose-rs device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            }))?;

        let size = window.inner_size();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb()) // pick sRGB if available
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

        // Pipelines: Rects
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/rect.wgsl"))),
        });
        let rect_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect bind layout"),
            entries: &[],
        });
        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect pipeline layout"),
            bind_group_layouts: &[], //&[&rect_bind_layout],
            push_constant_ranges: &[],
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
                        // xywh: vec4<f32>
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        // radius: f32
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32,
                        },
                        // color: vec4<f32>
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
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Pipelines: Borders (SDF ring)
        let border_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("border.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/border.wgsl"))),
        });
        let border_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("border bind layout"),
                entries: &[],
            });
        let border_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("border pipeline layout"),
                bind_group_layouts: &[], //&[&border_bind_layout],
                push_constant_ranges: &[],
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
                        // xywh
                        wgpu::VertexAttribute {
                            shader_location: 0,
                            offset: 0,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        // radius_outer
                        wgpu::VertexAttribute {
                            shader_location: 1,
                            offset: 16,
                            format: wgpu::VertexFormat::Float32,
                        },
                        // stroke
                        wgpu::VertexAttribute {
                            shader_location: 2,
                            offset: 20,
                            format: wgpu::VertexFormat::Float32,
                        },
                        // color
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Pipelines: Text
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
            bind_group_layouts: &[&text_bind_layout],
            push_constant_ranges: &[],
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Atlases
        let atlas_mask = Self::init_atlas_mask(&device)?;
        let atlas_color = Self::init_atlas_color(&device)?;

        Ok(Self {
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            // rect_bind_layout,
            border_pipeline,
            // border_bind_layout,
            text_pipeline_mask,
            text_pipeline_color,
            text_bind_layout,
            atlas_mask,
            atlas_color,
        })
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
            mipmap_filter: wgpu::FilterMode::Linear,
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
            mipmap_filter: wgpu::FilterMode::Linear,
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

    fn upload_glyph_mask(&mut self, key: compose_text::GlyphKey, px: u32) -> Option<GlyphInfo> {
        let keyp = (key, px);
        if let Some(info) = self.atlas_mask.map.get(&keyp) {
            return Some(*info);
        }

        let gb = compose_text::rasterize(key, px as f32)?;
        if gb.w == 0 || gb.h == 0 || gb.data.is_empty() {
            return None; //Whitespace, but doesn't get inserted?
        }
        if !matches!(
            gb.content,
            cosmic_text::SwashContent::Mask | cosmic_text::SwashContent::SubpixelMask
        ) {
            return None; // handled by color path
        }
        let w = gb.w.max(1);
        let h = gb.h.max(1);
        // Packing
        if self.atlas_mask.next_x + w + 1 >= self.atlas_mask.size {
            self.atlas_mask.next_x = 1;
            self.atlas_mask.next_y += self.atlas_mask.row_h + 1;
            self.atlas_mask.row_h = 0;
        }
        if self.atlas_mask.next_y + h + 1 >= self.atlas_mask.size {
            // atlas_mask full
            return None;
        }
        let x = self.atlas_mask.next_x;
        let y = self.atlas_mask.next_y;
        self.atlas_mask.next_x += w + 1;
        self.atlas_mask.row_h = self.atlas_mask.row_h.max(h + 1);

        let buf = gb.data;

        // Upload
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
            &buf,
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
            bearing_x: 0.0, // not used from atlas_mask so take it via shaping
            bearing_y: 0.0,
            advance: 0.0,
        };
        self.atlas_mask.map.insert(keyp, info);
        Some(info)
    }
    fn upload_glyph_color(&mut self, key: compose_text::GlyphKey, px: u32) -> Option<GlyphInfo> {
        let keyp = (key, px);
        if let Some(info) = self.atlas_color.map.get(&keyp) {
            return Some(*info);
        }
        let gb = compose_text::rasterize(key, px as f32)?;
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

    // Atlas alloc/grow (A8)
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
        // recreate texture
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
        // rebuild all keys
        let keys: Vec<(compose_text::GlyphKey, u32)> =
            self.atlas_mask.map.keys().copied().collect();
        self.atlas_mask.map.clear();
        for (k, px) in keys {
            let _ = self.upload_glyph_mask(k, px);
        }
    }
    // Atlas alloc/grow (RGBA)
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
        let keys: Vec<(compose_text::GlyphKey, u32)> =
            self.atlas_color.map.keys().copied().collect();
        self.atlas_color.map.clear();
        for (k, px) in keys {
            let _ = self.upload_glyph_color(k, px);
        }
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
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Helper: pixels -> NDC
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
        fn to_ndc_radius(r: f32, fb_w: f32, fb_h: f32) -> f32 {
            let rx = to_ndc_scalar(r, fb_w);
            let ry = to_ndc_scalar(r, fb_h);
            rx.min(ry)
        }
        fn to_ndc_stroke(w: f32, fb_w: f32, fb_h: f32) -> f32 {
            let sx = to_ndc_scalar(w, fb_w);
            let sy = to_ndc_scalar(w, fb_h);
            sx.min(sy)
        }
        fn to_scissor(r: &compose_core::Rect) -> (u32, u32, u32, u32) {
            let x = r.x.max(0.0).floor() as u32;
            let y = r.y.max(0.0).floor() as u32;
            let w = r.w.max(0.0).ceil() as u32;
            let h = r.h.max(0.0).ceil() as u32;
            (x, y, w.max(1), h.max(1))
        }

        let fb_w = self.config.width as f32;
        let fb_h = self.config.height as f32;

        let mut clip_stack: Vec<compose_core::Rect> = Vec::new();

        // Prebuild draw commands, batching per pipeline between clip boundaries
        enum Cmd {
            SetClipPush(compose_core::Rect),
            SetClipPop,
            Rect(wgpu::Buffer, u32),
            Border(wgpu::Buffer, u32),
            GlyphsMask(wgpu::Buffer, u32),
            GlyphsColor(wgpu::Buffer, u32),
        }
        let mut cmds: Vec<Cmd> = Vec::with_capacity(scene.nodes.len());
        struct Batch {
            rects: Vec<RectInstance>,
            borders: Vec<BorderInstance>,
            masks: Vec<GlyphInstance>,
            colors: Vec<GlyphInstance>,
        }
        impl Batch {
            fn new() -> Self {
                Self {
                    rects: vec![],
                    borders: vec![],
                    masks: vec![],
                    colors: vec![],
                }
            }
            fn flush(&mut self, dev: &wgpu::Device, cmds: &mut Vec<Cmd>) {
                if !self.rects.is_empty() {
                    let buf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rect batch"),
                        contents: bytemuck::cast_slice(&self.rects),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    cmds.push(Cmd::Rect(buf, self.rects.len() as u32));
                }
                if !self.borders.is_empty() {
                    let buf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("border batch"),
                        contents: bytemuck::cast_slice(&self.borders),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    cmds.push(Cmd::Border(buf, self.borders.len() as u32));
                }
                if !self.masks.is_empty() {
                    let buf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("glyph mask batch"),
                        contents: bytemuck::cast_slice(&self.masks),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    cmds.push(Cmd::GlyphsMask(buf, self.masks.len() as u32));
                }
                if !self.colors.is_empty() {
                    let buf = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("glyph color batch"),
                        contents: bytemuck::cast_slice(&self.colors),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    cmds.push(Cmd::GlyphsColor(buf, self.colors.len() as u32));
                }
            }
        }
        let mut batch = Batch::new();

        for node in &scene.nodes {
            match node {
                SceneNode::Rect {
                    rect,
                    color,
                    radius,
                } => {
                    batch.rects.push(RectInstance {
                        xywh: to_ndc(rect.x, rect.y, rect.w, rect.h, fb_w, fb_h),
                        radius: to_ndc_radius(*radius, fb_w, fb_h),
                        color: color.to_linear(),
                    });
                }
                SceneNode::Border {
                    rect,
                    color,
                    width,
                    radius,
                } => {
                    batch.borders.push(BorderInstance {
                        xywh: to_ndc(rect.x, rect.y, rect.w, rect.h, fb_w, fb_h),
                        radius_outer: to_ndc_radius(*radius, fb_w, fb_h),
                        stroke: to_ndc_stroke(*width, fb_w, fb_h),
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
                    // Shape line using compose-text (correct ligatures/bidi/fallback)
                    let shaped = compose_text::shape_line(text, px);
                    for sg in shaped {
                        // Try color first; if not color, try mask
                        if let Some(info) = self.upload_glyph_color(sg.key, px as u32) {
                            let x = rect.x + sg.x + sg.bearing_x;
                            let y = rect.y + sg.y - sg.bearing_y;
                            batch.colors.push(GlyphInstance {
                                xywh: to_ndc(x, y, info.w, info.h, fb_w, fb_h),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: [1.0, 1.0, 1.0, 1.0], // do not tint color glyphs
                            });
                        } else if let Some(info) = self.upload_glyph_mask(sg.key, px as u32) {
                            let x = rect.x + sg.x + sg.bearing_x;
                            let y = rect.y + sg.y - sg.bearing_y;
                            batch.masks.push(GlyphInstance {
                                xywh: to_ndc(x, y, info.w, info.h, fb_w, fb_h),
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                            });
                        }
                    }
                }
                SceneNode::PushClip { rect, .. } => {
                    batch.flush(&self.device, &mut cmds);
                    cmds.push(Cmd::SetClipPush(*rect));
                }
                SceneNode::PopClip => {
                    batch.flush(&self.device, &mut cmds);
                    cmds.push(Cmd::SetClipPop);
                }
            }
            // flush trailing batch
            batch.flush(&self.device, &mut cmds);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // initial full scissor
            rpass.set_scissor_rect(0, 0, self.config.width, self.config.height);
            let bind_mask = self.atlas_bind_group_mask();
            let bind_color = self.atlas_bind_group_color();
            let mut clip_stack: Vec<compose_core::Rect> = Vec::new();

            for cmd in cmds {
                match cmd {
                    Cmd::SetClipPush(r) => {
                        clip_stack.push(r);
                        // compute intersection of all active clips
                        let mut acc = r;
                        for c in &clip_stack {
                            let x0 = acc.x.max(c.x);
                            let y0 = acc.y.max(c.y);
                            let x1 = (acc.x + acc.w).min(c.x + c.w);
                            let y1 = (acc.y + acc.h).min(c.y + c.h);
                            acc = compose_core::Rect {
                                x: x0,
                                y: y0,
                                w: (x1 - x0).max(0.0),
                                h: (y1 - y0).max(0.0),
                            };
                        }
                        let (x, y, w, h) = to_scissor(&acc);
                        rpass.set_scissor_rect(x, y, w, h);
                    }
                    Cmd::SetClipPop => {
                        let _ = clip_stack.pop();
                        if let Some(top) = clip_stack.last() {
                            let (x, y, w, h) = to_scissor(top);
                            rpass.set_scissor_rect(x, y, w, h);
                        } else {
                            rpass.set_scissor_rect(0, 0, self.config.width, self.config.height);
                        }
                    }
                    Cmd::Rect(buf, n) => {
                        rpass.set_pipeline(&self.rect_pipeline);
                        rpass.set_vertex_buffer(0, buf.slice(..));
                        rpass.draw(0..6, 0..n);
                    }
                    Cmd::Border(buf, n) => {
                        rpass.set_pipeline(&self.border_pipeline);
                        rpass.set_vertex_buffer(0, buf.slice(..));
                        rpass.draw(0..6, 0..n);
                    }
                    Cmd::GlyphsMask(buf, n) => {
                        rpass.set_pipeline(&self.text_pipeline_mask);
                        rpass.set_bind_group(0, &bind_mask, &[]);
                        rpass.set_vertex_buffer(0, buf.slice(..));
                        rpass.draw(0..6, 0..n);
                    }
                    Cmd::GlyphsColor(buf, n) => {
                        rpass.set_pipeline(&self.text_pipeline_color);
                        rpass.set_bind_group(0, &bind_color, &[]);
                        rpass.set_vertex_buffer(0, buf.slice(..));
                        rpass.draw(0..6, 0..n);
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| frame.present())) {
            log::warn!("frame.present panicked: {:?}", e);
        }
    }
}

fn to_scissor(r: &compose_core::Rect) -> (u32, u32, u32, u32) {
    let x = r.x.max(0.0).floor() as u32;
    let y = r.y.max(0.0).floor() as u32;
    let w = r.w.max(0.0).ceil() as u32;
    let h = r.h.max(0.0).ceil() as u32;
    (x, y, w.max(1), h.max(1))
}
