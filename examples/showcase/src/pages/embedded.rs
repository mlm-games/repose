use repose_canvas::Embedded;
use repose_core::PaintCallbackInfo;
use repose_core::prelude::*;
use repose_render_wgpu::{Callback, CallbackResources, ScreenDescriptor, WgpuCallback};
use repose_ui::*;

use crate::ui::{Hint, Page, Section, sp};

struct TriangleResources {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct DemoTriangle {
    angle: Vec2, // x=yaw (dx), y=pitch (dy)
}

impl WgpuCallback for DemoTriangle {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _encoder: &mut wgpu::CommandEncoder,
        screen: &ScreenDescriptor,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Lazily create pipeline + buffers on first prepare (partial clone of the egui demo).
        if resources.get::<TriangleResources>().is_none() {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("demo_triangle"),
                source: wgpu::ShaderSource::Wgsl(
                    r#"
                    struct Uniforms { angles: vec2<f32>, _pad: vec2<f32>, }
                    @group(0) @binding(0) var<uniform> uniforms: Uniforms;
                    struct VsIn { @location(0) pos: vec2<f32>, @location(1) color: vec3<f32> }
                    struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) color: vec3<f32> }
                    @vertex
                    fn vs_main(in: VsIn) -> VsOut {
                        let ax = uniforms.angles.x; // yaw
                        let ay = uniforms.angles.y; // pitch
                        let cx = cos(ay); let sx = sin(ay);
                        let cy = cos(ax); let sy = sin(ax);
                        var p = vec3<f32>(in.pos.x, in.pos.y, 0.0);
                        var p1 = vec3<f32>(p.x * cy - p.z * sy, p.y, p.x * sy + p.z * cy);
                        var p2 = vec3<f32>(p1.x, p1.y * cx - p1.z * sx, p1.y * sx + p1.z * cx);
                        let d: f32 = 1.7;
                        let persp = d / (d - p2.z * 0.9);
                        let proj = p2.xy * persp * 0.95;
                        var out: VsOut;
                        out.pos = vec4<f32>(proj, 0.0, 1.0);
                        let light = clamp(0.65 + 0.35 * cos(ax * 0.6) * cos(ay * 0.6), 0.55, 1.0);
                        out.color = in.color * light;
                        return out;
                    }
                    @fragment
                    fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
                        return vec4<f32>(in.color, 1.0);
                    }
                    "#
                    .into(),
                ),
            });

            let vertices: &[f32] = &[
                0.0, 0.68, 0.98, 0.45, 0.32, -0.58, -0.42, 0.32, 0.78, 0.96, 0.58, -0.42, 0.42,
                0.96, 0.52,
            ];
            let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("demo_triangle_vb"),
                size: std::mem::size_of_val(vertices) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(vertices));

            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("demo_triangle_uniform"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("demo_triangle_bgl"),
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

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("demo_triangle_bg"),
                layout: &bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("demo_triangle_pl"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("demo_triangle_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 20, // 2*4 pos + 3*4 color
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 8,
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
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
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
                }),
                multisample: wgpu::MultisampleState {
                    count: screen.sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });

            resources.insert(TriangleResources {
                pipeline,
                vertex_buffer,
                uniform_buffer,
                bind_group,
            });
        }

        if let Some(res) = resources.get::<TriangleResources>() {
            let mut raw = [0f32; 8];
            raw[0] = self.angle.x;
            raw[1] = self.angle.y;
            queue.write_buffer(&res.uniform_buffer, 0, bytemuck::cast_slice(&raw));
        } else {
            log::warn!("DemoTriangle: resources missing after init");
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: PaintCallbackInfo,
        rpass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(res) = resources.get::<TriangleResources>() {
            rpass.set_pipeline(&res.pipeline);
            rpass.set_bind_group(0, &res.bind_group, &[]);
            rpass.set_vertex_buffer(0, res.vertex_buffer.slice(..));
            rpass.draw(0..3, 0..1);
        }
    }
}

pub fn screen() -> View {
    let angle = remember_mutable(|| Vec2 { x: 0.2, y: 0.25 });
    let drag_pos = remember_mutable(|| Vec2 { x: 0.0, y: 0.0 });
    let is_dragging = remember_mutable(|| false);

    let payload = {
        let a = *angle.get();
        Callback::new(DemoTriangle { angle: a })
    };

    Page(vec![
        Section(
            "Interactive 3D (wgpu callback)",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Drag in any direction (directly scooped from egui, though bottom-up rotation wouldn't work as intended for the same reason) horizontal = yaw, vertical = pitch. prepare uploads vec2 uniform, paint draws with perspective + per-vertex gradient. Viewport = layout rect.")
                    .color(theme().on_surface_variant)
                    .size(12.0),
                Row(Modifier::new().gap(sp::SM)).child((
                    Text(format!("yaw {:.2} pitch {:.2} rad", angle.get().x, angle.get().y)),
                    Text(format!("drag ({:.0}, {:.0})", drag_pos.get().x, drag_pos.get().y))
                        .color(theme().on_surface_variant)
                        .size(12.0),
                )),
                Embedded(
                    Modifier::new()
                        .size(560.0, 220.0)
                        .background(theme().surface_container_low)
                        .border(1.0, theme().outline_variant, 16.0)
                        .clip_rounded(16.0)
                        .on_pointer_down({
                            let drag_pos = drag_pos.clone();
                            let is_dragging = is_dragging.clone();
                            move |ev: PointerEvent| {
                                is_dragging.set(true);
                                drag_pos.set(ev.position);
                                log::info!("down {:?}", ev.position);
                            }
                        })
                        .on_pointer_up({
                            let is_dragging = is_dragging.clone();
                            move |_ev: PointerEvent| {
                                is_dragging.set(false);
                            }
                        })
                        .on_pointer_move({
                            let angle = angle.clone();
                            let drag_pos = drag_pos.clone();
                            let is_dragging = is_dragging.clone();
                            move |ev: PointerEvent| {
                                if !*is_dragging.get() {
                                    return;
                                }
                                let dx = ev.position.x - drag_pos.get().x;
                                let dy = ev.position.y - drag_pos.get().y;
                                drag_pos.set(ev.position);
                                angle.update(|a| {
                                    a.x = (a.x + dx * 0.01) % (std::f32::consts::TAU);
                                    a.y = (a.y + dy * 0.01).clamp(-1.3, 1.3);
                                });
                                request_frame();
                            }
                        }),
                    payload,
                ),
            )),
        ),

    ])
}
