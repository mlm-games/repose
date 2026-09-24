//! Headless probe for [`DepthComposite`](repose_render_wgpu::DepthComposite).

use repose_core::{Color, Rect, Scene, SceneNode};
use repose_render_wgpu::{
    Callback, CallbackRenderPass, CallbackResources, DepthComposite, ScreenDescriptor,
    WgpuCallback, offscreen::OffscreenRenderer,
};

const SHADER: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = camera.view_proj * vec4<f32>(pos, 1.0);
    out.color = color;
    return out;
}
@fragment
fn fs_main(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(color, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vert {
    pos: [f32; 3],
    color: [f32; 3],
}

/// Two screen-filling quads: far red pushed first, near green second.
/// Drawn into the composite's offscreen target with real depth.
struct Probe;

impl WgpuCallback for Probe {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        screen: &ScreenDescriptor,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        DepthComposite::get(resources).ensure(device, screen, "test.composite", 64, 64);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("test-composite-scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test-composite-cam"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Identity view-projection: NDC in, NDC out. Far quad at z=0.9,
        // near quad at z=0.1 — depth must resolve to near regardless of
        // push order.
        let ident: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        queue.write_buffer(&camera, 0, bytemuck::cast_slice(&[ident]));
        let cam_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test-composite-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let cam_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test-composite-bg"),
            layout: &cam_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("test-composite-pl"),
            bind_group_layouts: &[Some(&cam_layout)],
            immediate_size: 0,
        });
        // Identity camera must be a plain local (borrow ends before pass).
        let depth_format = DepthComposite::get(resources)
            .depth_format("test.composite")
            .unwrap_or(wgpu::TextureFormat::Depth24PlusStencil8);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("test-composite-pipe"),
            layout: Some(&pipe_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vert>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: screen.target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        // Fullscreen quads in NDC (CCW): far red first, near green second.
        let quad = |z: f32, c: [f32; 3]| {
            [
                Vert {
                    pos: [-1.0, -1.0, z],
                    color: c,
                },
                Vert {
                    pos: [1.0, -1.0, z],
                    color: c,
                },
                Vert {
                    pos: [1.0, 1.0, z],
                    color: c,
                },
                Vert {
                    pos: [-1.0, -1.0, z],
                    color: c,
                },
                Vert {
                    pos: [1.0, 1.0, z],
                    color: c,
                },
                Vert {
                    pos: [-1.0, 1.0, z],
                    color: c,
                },
            ]
        };
        let verts: Vec<Vert> = quad(0.9, [1.0, 0.0, 0.0])
            .into_iter()
            .chain(quad(0.1, [0.0, 1.0, 0.0]))
            .collect();
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test-composite-verts"),
            size: (verts.len() * size_of::<Vert>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&verts));
        let composite = DepthComposite::get(resources);
        if let Some(mut pass) =
            composite.begin_scene("test.composite", encoder, [0.0, 0.0, 0.0, 1.0])
        {
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &cam_bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..12, 0..1);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: repose_core::PaintCallbackInfo,
        rpass: &mut CallbackRenderPass<'_, '_>,
        resources: &CallbackResources,
    ) {
        if let Some(composite) = resources.get::<DepthComposite>() {
            composite.blit("test.composite", rpass);
        }
    }
}

#[test]
fn depth_composite_resolves_near_over_far() {
    let mut renderer = match OffscreenRenderer::new_blocking(64, 64, 1) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("SKIP composite test (no GPU): {e}");
            return;
        }
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 255),
        nodes: vec![SceneNode::Callback {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 64.0,
                h: 64.0,
            },
            payload: Callback::new(Probe),
        }],
    };
    let px = renderer
        .render_rgba(&scene, Some([0.0, 0.0, 0.0, 1.0]))
        .expect("render");
    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * 64 + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    };
    assert_eq!(at(32, 32), [0, 255, 0, 255], "near quad wins by depth");
    assert_eq!(at(4, 4), [0, 255, 0, 255], "near quad fills the view");
}
