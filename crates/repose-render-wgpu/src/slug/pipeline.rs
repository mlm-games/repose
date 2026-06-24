use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};

use crate::slug::cache::GlyphSlugCache;

/// Per-instance vertex data for Slug-rendered glyphs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct SlugVertex {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// Offset in u32 units into the glyph data storage buffer.
    pub glyph_base: u32,
    /// Glyph baseline origin in screen pixel space.
    pub origin_px: [f32; 2],
    /// Rotation pivot in screen pixel space.
    pub pivot_px: [f32; 2],
    /// Rotation: cos(angle), sin(angle). [1.0, 0.0] for unrotated.
    pub cos_sin: [f32; 2],
}

/// Create the bind group layout for the slug storage buffer (shared across sample counts).
pub fn create_storage_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("slug storage layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: Some(wgpu::BufferSize::new(16).unwrap()),
            },
            count: None,
        }],
    })
}

/// Create a slug render pipeline for a given sample count.
pub fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
    globals_layout: &wgpu::BindGroupLayout,
    storage_layout: &wgpu::BindGroupLayout,
    stencil: &wgpu::DepthStencilState,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("slug.wgsl"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../shaders/slug.wgsl"))),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("slug pipeline layout"),
        bind_group_layouts: &[Some(globals_layout), Some(storage_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("slug pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<SlugVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        shader_location: 0,
                        offset: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 1,
                        offset: 8,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 2,
                        offset: 16,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 3,
                        offset: 32,
                        format: wgpu::VertexFormat::Uint32,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 4,
                        offset: 36,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 5,
                        offset: 44,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        shader_location: 6,
                        offset: 52,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(stencil.clone()),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

/// GPU storage for vector glyph curve data (shared across sample counts).
pub struct SlugPipeline {
    storage_buf: wgpu::Buffer,
    storage_bind: wgpu::BindGroup,
    storage_layout: wgpu::BindGroupLayout,
}

impl SlugPipeline {
    pub fn new(device: &wgpu::Device, storage_layout: &wgpu::BindGroupLayout) -> Self {
        let storage_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slug storage"),
            size: 262144,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let storage_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("slug storage bind"),
            layout: storage_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: storage_buf.as_entire_binding(),
            }],
        });

        Self {
            storage_buf,
            storage_bind,
            storage_layout: storage_layout.clone(),
        }
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut GlyphSlugCache,
        prev_generation: &mut u64,
    ) {
        let data = cache.build_storage();
        let needed_bytes = (data.len() as u64) * 4;
        if needed_bytes > self.storage_buf.size() {
            let new_size = (needed_bytes * 2).next_power_of_two().max(262144);
            let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("slug storage (grown)"),
                size: new_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.storage_buf = new_buf;
            self.storage_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("slug storage bind"),
                layout: &self.storage_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.storage_buf.as_entire_binding(),
                }],
            });
            *prev_generation = !0;
        }
        if !data.is_empty() && cache.generation != *prev_generation {
            queue.write_buffer(&self.storage_buf, 0, bytemuck::cast_slice(&data));
            *prev_generation = cache.generation;
        }
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.storage_bind
    }
}
