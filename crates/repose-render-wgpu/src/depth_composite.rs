//! Depth-capable scene composite for [`WgpuCallback`](super::WgpuCallback)s.
//!
//! The shared UI pass this crate executes carries stencil but no depth ops
//! (`depth_ops: None`), so pipelines that write depth are rejected inside
//! it by validation. This module is the supported path for depth-tested
//! content (3D viewports): render the scene into a caller-owned offscreen
//! target (with its own depth texture) during `prepare`, then draw the
//! target back as a fullscreen triangle in `paint`.
//!
//! One [`DepthComposite`] per viewport id (same ownership split as the
//! sprite batch in `repame-sprite`): each id owns its target + pipelines,
//! so overlapping viewports never share depth. The blit honors the UI
//! stencil contract (`LessEqual`) and touches no depth, so clips keep
//! working while depth stays viewport-local.

use std::collections::HashMap;

use super::{CallbackResources, ScreenDescriptor};

/// Fullscreen textured triangle: samples the offscreen scene 1:1.
const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_smp: sampler;
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i / 2u) * 4.0 - 1.0;
    let y = f32(i % 2u) * 4.0 - 1.0;
    var out: VsOut;
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(scene_tex, scene_smp, in.uv);
}
"#;

/// Viewport-owned offscreen scene target + depth buffer + blit pipeline.
///
/// Created through [`DepthComposite::ensure`] (per viewport id), drawn into
/// with [`DepthComposite::begin_scene`], composited back with
/// [`DepthComposite::blit`]. Textures recreate on format/sample/size
/// change; buffer contents are per-frame and never retained.
#[derive(Default)]
pub struct DepthComposite {
    targets: HashMap<String, Target>,
}

struct Target {
    key: (wgpu::TextureFormat, u32, u32, u32),
    #[allow(dead_code)]
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    #[allow(dead_code)]
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bind: wgpu::BindGroup,
}

impl DepthComposite {
    /// Fetch the composite store from callback resources (creating it on
    /// first use). One store per render pass; ids disambiguate viewports.
    pub fn get(resources: &mut CallbackResources) -> &mut Self {
        resources.get_or_insert_with::<Self>()
    }

    /// Ensure the offscreen target for `id` at `w`x`h` (recreates on
    /// format/sample/size change, like every other viewport-owned target).
    /// Dimensions clamp to >= 1.
    #[allow(clippy::too_many_arguments)] // (device, screen, id, w, h) — mirrors ensure_resources conventions
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        screen: &ScreenDescriptor,
        id: &str,
        w: u32,
        h: u32,
    ) {
        let w = w.max(1);
        let h = h.max(1);
        let key = (screen.target_format, screen.sample_count, w, h);
        if self.targets.get(id).is_some_and(|t| t.key == key) {
            return;
        }
        let depth_format = wgpu::TextureFormat::Depth24PlusStencil8;
        let scene = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_composite_scene"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: screen.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_composite_depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth_composite_blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth_composite_blit_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("depth_composite_blit_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 1.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth_composite_blit_bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth_composite_blit_pl"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth_composite_blit"),
            layout: Some(&pipe_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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
            // depth ops disabled, so the blit never disturbs UI depth/stencil.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState {
                    front: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::LessEqual,
                        ..Default::default()
                    },
                    back: wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::LessEqual,
                        ..Default::default()
                    },
                    ..Default::default()
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
        self.targets.insert(
            id.to_string(),
            Target {
                key,
                scene,
                scene_view,
                depth,
                depth_view,
                depth_format,
                blit_pipeline,
                blit_bind: bind,
            },
        );
    }

    /// Begin the offscreen scene pass for `id` (clearing color to `clear`
    /// and depth to 1.0). The caller issues its depth-tested draws inside
    /// the returned pass, then ends it; [`blit`](Self::blit) composites in
    /// `paint`. Returns `false` when `id` has no target (call [`ensure`](Self::ensure) first).
    pub fn begin_scene<'a>(
        &'a self,
        id: &str,
        encoder: &'a mut wgpu::CommandEncoder,
        clear: [f32; 4],
    ) -> Option<wgpu::RenderPass<'a>> {
        let t = self.targets.get(id)?;
        let (w, h) = (t.key.2 as f32, t.key.3 as f32);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("depth_composite_scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear[0] as f64,
                        g: clear[1] as f64,
                        b: clear[2] as f64,
                        a: clear[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &t.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, w, h, 0.0, 1.0);
        Some(pass)
    }

    /// Depth format backing the scene target (for caller pipelines).
    pub fn depth_format(&self, id: &str) -> Option<wgpu::TextureFormat> {
        self.targets.get(id).map(|t| t.depth_format)
    }

    /// Composite the offscreen scene for `id` into the main pass. The
    /// renderer has already set the viewport to the callback rect, which
    /// matches the offscreen texture 1:1 (both come from the painted frame
    /// geometry). No-op when `id` has no target.
    pub fn blit(&self, id: &str, rpass: &mut wgpu::RenderPass<'_>) {
        let Some(t) = self.targets.get(id) else {
            return;
        };
        rpass.set_pipeline(&t.blit_pipeline);
        rpass.set_bind_group(0, &t.blit_bind, &[]);
        rpass.draw(0..3, 0..1);
    }
}
