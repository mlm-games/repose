use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use ab_glyph::{point, Font, FontArc, Glyph, PxScale, ScaleFont};
use compose_core::{Color, GlyphRasterConfig, RenderBackend, Scene, SceneNode};
use fontdb::Database;
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
    text_pipeline: wgpu::RenderPipeline,
    text_bind_layout: wgpu::BindGroupLayout,

    // Glyph atlas
    font: FontArc,
    // font_size: f32,
    atlas: Atlas,
}

struct Atlas {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    next_x: u32,
    next_y: u32,
    row_h: u32,
    map: HashMap<(char, u32), GlyphInfo>,
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
        .map_err(|e| anyhow::anyhow!("No adapter"))?;

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
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/text.wgsl"))),
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
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
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
                module: &text_shader,
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

        // Font and atlas
        let (font, atlas) = Self::init_font_and_atlas(&device)?;

        Ok(Self {
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            // rect_bind_layout,
            border_pipeline,
            // border_bind_layout,
            text_pipeline,
            text_bind_layout,
            font,
            // font_size: 24.0,
            atlas,
        })
    }

    fn init_font_and_atlas(device: &wgpu::Device) -> anyhow::Result<(FontArc, Atlas)> {
        // Load default sans-serif from system
        let mut db = Database::new();
        db.load_system_fonts();

        let query = fontdb::Query {
            families: &[fontdb::Family::SansSerif],
            ..Default::default()
        };
        let id = db
            .query(&query)
            .ok_or_else(|| anyhow::anyhow!("No system sans-serif font found"))?;

        let (source, _face_index) = db
            .face_source(id)
            .ok_or_else(|| anyhow::anyhow!("Font face not found"))?;

        let font = match source {
            fontdb::Source::Binary(data) => {
                let bytes: &[u8] = data.as_ref().as_ref();
                FontArc::try_from_vec(bytes.to_vec())
                    .map_err(|_| anyhow::anyhow!("Failed to load font from binary data"))?
            }
            fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                let bytes = std::fs::read(path)?;
                FontArc::try_from_vec(bytes)
                    .map_err(|_| anyhow::anyhow!("Failed to load font from file"))?
            }
        };

        let size = 1024u32;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
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
            label: Some("glyph atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok((
            font,
            Atlas {
                tex,
                view,
                sampler,
                size,
                next_x: 1,
                next_y: 1,
                row_h: 0,
                map: HashMap::new(),
            },
        ))
    }

    fn atlas_bind_group(&self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bind"),
            layout: &self.text_bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas.sampler),
                },
            ],
        })
    }

    fn upload_glyph(&mut self, ch: char, px: u32) -> Option<GlyphInfo> {
        let key = (ch, px);
        if let Some(info) = self.atlas.map.get(&key) {
            return Some(*info);
        }

        let scaled = self.font.as_scaled(PxScale::from(px as f32));

        let glyph_id = scaled.glyph_id(ch);
        let glyph = glyph_id.with_scale_and_position(PxScale::from(px as f32), point(0.0, 0.0));

        let outlined = scaled.outline_glyph(glyph)?;
        let bb = outlined.px_bounds();

        let w = (bb.max.x - bb.min.x).ceil().max(1.0) as u32;
        let h = (bb.max.y - bb.min.y).ceil().max(1.0) as u32;

        // Packing
        if self.atlas.next_x + w + 1 >= self.atlas.size {
            self.atlas.next_x = 1;
            self.atlas.next_y += self.atlas.row_h + 1;
            self.atlas.row_h = 0;
        }
        if self.atlas.next_y + h + 1 >= self.atlas.size {
            // atlas full
            return None;
        }
        let x = self.atlas.next_x;
        let y = self.atlas.next_y;
        self.atlas.next_x += w + 1;
        self.atlas.row_h = self.atlas.row_h.max(h + 1);

        let mut buf = vec![0u8; (w * h) as usize];
        outlined.draw(|gx, gy, cov| {
            let idx = (gy as u32 * w + gx as u32) as usize;
            if idx < buf.len() {
                buf[idx] = (cov * 255.0) as u8;
            }
        });

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
                texture: &self.atlas.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &buf,
            layout,
            size,
        );

        let h_metrics = scaled.h_advance(glyph_id);

        let info = GlyphInfo {
            u0: x as f32 / self.atlas.size as f32,
            v0: y as f32 / self.atlas.size as f32,
            u1: (x + w) as f32 / self.atlas.size as f32,
            v1: (y + h) as f32 / self.atlas.size as f32,
            w: w as f32,
            h: h as f32,
            bearing_x: bb.min.x,
            bearing_y: -bb.min.y,
            advance: h_metrics,
        };
        self.atlas.map.insert(key, info);
        Some(info)
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
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                self.surface
                    .get_current_texture()
                    .expect("failed to acquire frame")
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

        // Collect instances
        let mut rects: Vec<RectInstance> = vec![];
        let mut borders: Vec<BorderInstance> = vec![];
        let mut glyphs: Vec<GlyphInstance> = vec![];

        let fb_w = self.config.width as f32;
        let fb_h = self.config.height as f32;

        for node in &scene.nodes {
            match node {
                SceneNode::Rect {
                    rect,
                    color,
                    radius,
                } => {
                    rects.push(RectInstance {
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
                    borders.push(BorderInstance {
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
                    let px = (*size).clamp(8.0, 96.0) as u32;
                    let mut pen_x = rect.x;
                    let baseline = rect.y + size * 0.9;
                    for ch in text.chars() {
                        if ch == '\n' {
                            pen_x = rect.x;
                            continue;
                        }
                        if let Some(info) = self.upload_glyph(ch, px) {
                            let x = pen_x + info.bearing_x;
                            let y = baseline - info.bearing_y;
                            glyphs.push(GlyphInstance {
                                xywh: to_ndc(x, y, info.w, info.h, fb_w, fb_h),
                                // flip V to match bottom→top NDC extents
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                            });
                            pen_x += info.advance;
                        }
                    }
                }
            }
        }

        // Buffers
        let rect_buf = if !rects.is_empty() {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rect instances"),
                        contents: bytemuck::cast_slice(&rects),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        } else {
            None
        };

        let border_buf = if !borders.is_empty() {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("border instances"),
                        contents: bytemuck::cast_slice(&borders),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        } else {
            None
        };

        let glyph_buf = if !glyphs.is_empty() {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("glyph instances"),
                        contents: bytemuck::cast_slice(&glyphs),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        } else {
            None
        };

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

            if let Some(buf) = &rect_buf {
                rpass.set_pipeline(&self.rect_pipeline);
                rpass.set_vertex_buffer(0, buf.slice(..));
                rpass.draw(0..6, 0..rects.len() as u32);
            }

            if let Some(buf) = &border_buf {
                rpass.set_pipeline(&self.border_pipeline);
                rpass.set_vertex_buffer(0, buf.slice(..));
                rpass.draw(0..6, 0..borders.len() as u32);
            }

            if let Some(buf) = &glyph_buf {
                let bind = self.atlas_bind_group();
                rpass.set_pipeline(&self.text_pipeline);
                rpass.set_bind_group(0, &bind, &[]);
                rpass.set_vertex_buffer(0, buf.slice(..));
                rpass.draw(0..6, 0..glyphs.len() as u32);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
