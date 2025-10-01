use std::borrow::Cow;
use std::collections::HashMap;

use ab_glyph::{Font, FontArc, ScaleFont, PxScaleFont, Glyph, point};
use compose_core::{RenderBackend, Scene, SceneNode, GlyphRasterConfig, Color};
use fontdb::Database;
use wgpu::util::DeviceExt;

pub struct WgpuBackend {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,

    rect_pipeline: wgpu::RenderPipeline,
    rect_bind_layout: wgpu::BindGroupLayout,

    text_pipeline: wgpu::RenderPipeline,
    text_bind_layout: wgpu::BindGroupLayout,

    // Glyph atlas
    font: PxScaleFont<FontArc>,
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
    map: HashMap<(char, u32), GlyphInfo>, // (ch, px) -> glyph info
}

#[derive(Clone, Copy)]
struct GlyphInfo {
    u0: f32, v0: f32, u1: f32, v1: f32,
    w: f32, h: f32,
    bearing_x: f32, bearing_y: f32,
    advance: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectInstance {
    // x, y, w, h, radius
    xywh_r: [f32; 5],
    // rgba
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    // x, y, w, h
    xywh: [f32; 4],
    // uv
    uv: [f32; 4],
    // color
    color: [f32; 4],
}

impl WgpuBackend {
    pub fn new(window: &winit::window::Window) -> anyhow::Result<Self> {
        // Instance/Surface
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
        });

        let surface = unsafe { instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_window(window)?) }?;

        // Adapter/Device
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })).ok_or_else(|| anyhow::anyhow!("No adapter"))?;

        let caps = surface.get_capabilities(&adapter);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("compose-rs device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))?;

        let size = window.inner_size();
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Pipelines
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
            bind_group_layouts: &[&rect_bind_layout],
            push_constant_ranges: &[],
        });
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: "vs_main",
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<RectInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 0, offset: 0, format: wgpu::VertexFormat::Float32x5 },
                            wgpu::VertexAttribute { shader_location: 1, offset: 20, format: wgpu::VertexFormat::Float32x4 },
                        ],
                    }
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Text pipeline
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
                        multisampled: false, view_dimension: wgpu::TextureViewDimension::D2, sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
                entry_point: "vs_main",
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GlyphInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { shader_location: 0, offset: 0,  format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { shader_location: 1, offset: 16, format: wgpu::VertexFormat::Float32x4 },
                            wgpu::VertexAttribute { shader_location: 2, offset: 32, format: wgpu::VertexFormat::Float32x4 },
                        ],
                    }
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Font and atlas
        let (font, atlas) = Self::init_font_and_atlas(&device, &queue)?;

        Ok(Self {
            surface, device, queue, config,
            rect_pipeline, rect_bind_layout,
            text_pipeline, text_bind_layout,
            font, atlas,
        })
    }

    fn init_font_and_atlas(device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<(PxScaleFont<FontArc>, Atlas)> {
        // Load a default "sans-serif" from system
        let mut db = Database::new();
        db.load_system_fonts();
        let id = db.query(&fontdb::Query { families: &[fontdb::Family::SansSerif], ..Default::default() })
            .and_then(|m| m.first().cloned()).ok_or_else(|| anyhow::anyhow!("No system sans-serif font found"))?;
        let face = db.face(id).ok_or_else(|| anyhow::anyhow!("Font face not found"))?;
        let font = FontArc::try_from_slice(&face.data).or_else(|_| {
            // As a fallback, try the path on disk if available
            if let Some(path) = &face.source.as_path() {
                std::fs::read(path).ok().and_then(|b| FontArc::try_from_vec(b).ok()).ok_or(())
            } else { Err(()) }
        }).map_err(|_| anyhow::anyhow!("Failed to load font data"))?;

        let scaled = font.into_scaled(24.0);

        let size = 1024u32;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
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

        Ok((scaled, Atlas {
            tex, view, sampler,
            size, next_x: 1, next_y: 1, row_h: 0,
            map: HashMap::new(),
        }))
    }

    fn atlas_bind_group(&self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas bind"),
            layout: &self.text_bind_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.atlas.view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.atlas.sampler) },
            ],
        })
    }

    fn upload_glyph(&mut self, ch: char, px: u32) -> Option<GlyphInfo> {
        let key = (ch, px);
        if let Some(info) = self.atlas.map.get(&key) { return Some(*info); }

        let scaled = self.font.as_scaled(px as f32);
        let glyph = scaled.glyph_id(ch).with_scale(px as f32).with_position(point(0.0, 0.0));
        let bb = scaled.outline_glyph(glyph)?.pixel_bounding_box()?;
        let w = (bb.max.x - bb.min.x).max(1) as u32;
        let h = (bb.max.y - bb.min.y).max(1) as u32;

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

        let mut buf = vec![0u8; (w*h) as usize];
        if let Some(out) = scaled.outline_glyph(glyph) {
            out.draw(|gx, gy, cov| {
                let idx = (gy as u32 * w + gx as u32) as usize;
                buf[idx] = (cov * 255.0) as u8;
            });
        }

        // Upload
        let layout = wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(std::num::NonZeroU32::new(w).unwrap()),
            rows_per_image: None,
        };
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.atlas.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &buf,
            layout,
            size,
        );

        let h_ = h as f32;
        let info = GlyphInfo {
            u0: x as f32 / self.atlas.size as f32,
            v0: y as f32 / self.atlas.size as f32,
            u1: (x + w) as f32 / self.atlas.size as f32,
            v1: (y + h) as f32 / self.atlas.size as f32,
            w: w as f32, h: h_ as f32,
            bearing_x: bb.min.x as f32,
            bearing_y: -bb.min.y as f32, // flip Y
            advance: scaled.h_advance(glyph.id),
        };
        self.atlas.map.insert(key, info);
        Some(info)
    }
}

impl RenderBackend for WgpuBackend {
    fn configure_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.config.width = width; self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn frame(&mut self, scene: &Scene, glyph_cfg: GlyphRasterConfig) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture().expect("failed to acquire frame")
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Collect instances
        let mut rects: Vec<RectInstance> = vec![];
        let mut glyphs: Vec<GlyphInstance> = vec![];

        for node in &scene.nodes {
            match node {
                SceneNode::Rect { rect, color, radius } => {
                    rects.push(RectInstance {
                        xywh_r: [rect.x, rect.y, rect.w, rect.h, *radius],
                        color: color.to_linear(),
                    });
                }
                SceneNode::Border { rect, color, width, radius } => {
                    // outer
                    rects.push(RectInstance {
                        xywh_r: [rect.x, rect.y, rect.w, rect.h, *radius],
                        color: color.to_linear(),
                    });
                    // inner "erase": simple hack by drawing inner rect with background color; real impl would use stencil
                    let bg = scene.clear_color.to_linear();
                    rects.push(RectInstance {
                        xywh_r: [rect.x + width, rect.y + width, rect.w - 2.0*width, rect.h - 2.0*width, (*radius - *width).max(0.0)],
                        color: [bg[0], bg[1], bg[2], 1.0],
                    });
                }
                SceneNode::Text { rect, text, color, size } => {
                    let px = glyph_cfg.px.max(8.0).min(96.0) as u32;
                    let mut pen_x = rect.x;
                    let baseline = rect.y + size * 0.9;
                    for ch in text.chars() {
                        if ch == '\n' { pen_x = rect.x; continue; }
                        if let Some(info) = self.upload_glyph(ch, px) {
                            let x = pen_x + info.bearing_x;
                            let y = baseline - info.bearing_y;
                            glyphs.push(GlyphInstance {
                                xywh: [x, y, info.w, info.h],
                                uv: [info.u0, info.v0, info.u1, info.v1],
                                color: color.to_linear(),
                            });
                            pen_x += info.advance;
                        }
                    }
                }
            }
        }

        // Buffers
        let rect_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rect instances"),
            contents: bytemuck::cast_slice(&rects),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let glyph_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glyph instances"),
            contents: bytemuck::cast_slice(&glyphs),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame encoder") });
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
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Rects
            if !rects.is_empty() {
                rpass.set_pipeline(&self.rect_pipeline);
                rpass.set_vertex_buffer(0, rect_buf.slice(..));
                rpass.draw(0..6, 0..rects.len() as u32);
            }

            // Text
            if !glyphs.is_empty() {
                let bind = self.atlas_bind_group();
                rpass.set_pipeline(&self.text_pipeline);
                rpass.set_bind_group(0, &bind, &[]);
                rpass.set_vertex_buffer(0, glyph_buf.slice(..));
                rpass.draw(0..6, 0..glyphs.len() as u32);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

// WGSL shaders embedded
// shaders/rect.wgsl
// Draws a full-screen unit quad and expands from instance attributes.
#[allow(dead_code)]
const _: &str = include_str!("shaders/rect.wgsl");
#[allow(dead_code)]
const _: &str = include_str!("shaders/text.wgsl");
