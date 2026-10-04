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
//! sprite batch in `repame-sprite`): each id owns its target, so
//! overlapping viewports never share depth. The blit honors the UI
//! stencil contract (`LessEqual`) and touches no depth, so clips keep
//! working while depth stays viewport-local. The blit composites over the
//! UI instead of replacing it, so a transparent scene leaves the interface
//! beneath it visible.

use std::collections::HashMap;

use super::{CallbackRenderPass, CallbackResources, ScreenDescriptor};

const MAX_DEPTH_RESOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DEPTH_TARGETS: usize = 64;
/// Frames a target survives without an `ensure`. Viewports are intermittent
/// (animation, collapse, occlusion); dropping the textures every frame an
/// embedder skips one forces a reallocation and a bind-group rebuild on the
/// next paint, and used to throw away a shared blit pipeline with them.
const TARGET_IDLE_FRAMES: u64 = 3;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

pub trait BlitRenderPass {
    fn set_blit_pipeline(&mut self, pipeline: &wgpu::RenderPipeline);
    fn set_blit_bind_group(&mut self, index: u32, bind_group: &wgpu::BindGroup, offsets: &[u32]);
    fn draw_blit_triangle(&mut self);
}

impl BlitRenderPass for wgpu::RenderPass<'_> {
    fn set_blit_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        self.set_pipeline(pipeline);
    }

    fn set_blit_bind_group(&mut self, index: u32, bind_group: &wgpu::BindGroup, offsets: &[u32]) {
        self.set_bind_group(index, bind_group, offsets);
    }

    fn draw_blit_triangle(&mut self) {
        self.draw(0..3, 0..1);
    }
}

impl BlitRenderPass for CallbackRenderPass<'_, '_> {
    fn set_blit_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        self.set_pipeline(pipeline);
    }

    fn set_blit_bind_group(&mut self, index: u32, bind_group: &wgpu::BindGroup, offsets: &[u32]) {
        self.set_bind_group(index, bind_group, offsets);
    }

    fn draw_blit_triangle(&mut self) {
        self.draw(0..3, 0..1);
    }
}

/// Fullscreen textured triangle: samples the offscreen scene 1:1.
///
/// The scene target holds straight (un-premultiplied) alpha, and the blit
/// composites over UI already painted this frame, so the fragment stage
/// premultiplies and the pipeline uses `PREMULTIPLIED_ALPHA_BLENDING`.
/// Opaque scenes (`a == 1`) pass through unchanged.
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
    let c = textureSample(scene_tex, scene_smp, in.uv);
    return vec4<f32>(c.rgb * c.a, c.a);
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
    shared: Option<BlitShared>,
    /// Blit pipelines depend only on `(target_format, sample_count)`, never
    /// on the viewport size, so they outlive the textures they sample.
    pipelines: HashMap<(wgpu::TextureFormat, u32), wgpu::RenderPipeline>,
    next_tick: u64,
    frame_index: u64,
    bytes_total: u64,
}

struct BlitShared {
    shader: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline_layout: wgpu::PipelineLayout,
}

struct Target {
    key: (wgpu::TextureFormat, u32, u32, u32),
    #[allow(dead_code)]
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    #[allow(dead_code)]
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
    last_used_tick: u64,
    last_used_frame: u64,
    bytes: u64,
}

impl DepthComposite {
    /// Fetch the composite store from callback resources (creating it on
    /// first use). One store per render pass; ids disambiguate viewports.
    pub fn get(resources: &mut CallbackResources) -> &mut Self {
        resources.get_or_insert_with::<Self>()
    }

    pub fn begin_frame(&mut self) {
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    pub fn end_frame(&mut self) {
        let frame_index = self.frame_index;
        let mut removed = 0u64;
        self.targets.retain(|_, target| {
            if frame_index.saturating_sub(target.last_used_frame) <= TARGET_IDLE_FRAMES {
                true
            } else {
                removed = removed.saturating_add(target.bytes);
                false
            }
        });
        self.bytes_total = self.bytes_total.saturating_sub(removed);
    }

    fn shared(&mut self, device: &wgpu::Device) -> &BlitShared {
        if self.shared.is_none() {
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
                lod_max_clamp: 0.0,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("depth_composite_blit_pl"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            self.shared = Some(BlitShared {
                shader,
                layout,
                sampler,
                pipeline_layout,
            });
        }
        self.shared.as_ref().expect("shared blit resources")
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> &wgpu::RenderPipeline {
        let pipeline_key = (target_format, sample_count);
        if !self.pipelines.contains_key(&pipeline_key) {
            let shared = self.shared(device);
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("depth_composite_blit"),
                layout: Some(&shared.pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shared.shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shared.shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        // Premultiplied source over the existing UI: a
                        // transparent scene composites instead of erasing
                        // whatever the UI pass already drew beneath it.
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
                    format: DEPTH_FORMAT,
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
                        read_mask: 0xFF,
                        write_mask: 0,
                    },
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: sample_count,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            });
            self.pipelines.insert(pipeline_key, pipeline);
        }
        &self.pipelines[&pipeline_key]
    }

    /// Ensure the offscreen target for `id` at `w`x`h` (recreates on
    /// format/sample/size change, like every other viewport-owned target).
    /// Dimensions clamp to >= 1. Errors name the reason the target could
    /// not be allocated; the other viewports are left untouched.
    #[allow(clippy::too_many_arguments)] // (device, screen, id, w, h) — mirrors ensure_resources conventions
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        screen: &ScreenDescriptor,
        id: &str,
        w: u32,
        h: u32,
    ) -> anyhow::Result<()> {
        let w = w.max(1);
        let h = h.max(1);
        let key = (screen.target_format, screen.sample_count, w, h);
        self.next_tick = self.next_tick.wrapping_add(1);
        let tick = self.next_tick;
        if let Some(target) = self.targets.get_mut(id)
            && target.key == key
        {
            target.last_used_tick = tick;
            target.last_used_frame = self.frame_index;
            return Ok(());
        }
        let scene_bytes = screen
            .target_format
            .theoretical_memory_footprint(wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            });
        let depth_bytes = DEPTH_FORMAT.theoretical_memory_footprint(wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        });
        let bytes = scene_bytes.saturating_add(depth_bytes);
        let max_dimension = device.limits().max_texture_dimension_2d;
        if w > max_dimension || h > max_dimension {
            anyhow::bail!(
                "depth composite {id}: {w}x{h} exceeds the device texture limit {max_dimension}"
            );
        }
        if bytes > MAX_DEPTH_RESOURCE_BYTES {
            anyhow::bail!(
                "depth composite {id}: {w}x{h} needs {bytes} bytes, over the {MAX_DEPTH_RESOURCE_BYTES} byte limit"
            );
        }
        // `id`'s own bytes are replaced, not added, so the budget only ever
        // evicts siblings — and never leaves `id` without a target.
        let replaced = self.targets.get(id).map_or(0, |old| old.bytes);
        while self
            .bytes_total
            .saturating_sub(replaced)
            .saturating_add(bytes)
            > MAX_DEPTH_RESOURCE_BYTES
        {
            let Some(victim) = self
                .targets
                .iter()
                .filter(|(key, _)| key.as_str() != id)
                .min_by_key(|(_, target)| target.last_used_tick)
                .map(|(key, _)| key.clone())
            else {
                anyhow::bail!(
                    "depth composite {id}: no room in the {MAX_DEPTH_RESOURCE_BYTES} byte budget"
                );
            };
            if let Some(old) = self.targets.remove(&victim) {
                self.bytes_total = self.bytes_total.saturating_sub(old.bytes);
            }
        }
        let shared = self.shared(device);
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
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth_composite_blit_bg"),
            layout: &shared.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shared.sampler),
                },
            ],
        });
        self.pipeline(device, screen.target_format, screen.sample_count);
        let target = Target {
            key,
            scene,
            scene_view,
            depth,
            depth_view,
            blit_bind: bind,
            last_used_tick: tick,
            last_used_frame: self.frame_index,
            bytes,
        };
        if let Some(old) = self.targets.insert(id.to_string(), target) {
            self.bytes_total = self.bytes_total.saturating_sub(old.bytes);
        }
        self.bytes_total = self.bytes_total.saturating_add(bytes);
        while self.targets.len() > MAX_DEPTH_TARGETS {
            let Some(victim) = self
                .targets
                .iter()
                .filter(|(key, _)| key.as_str() != id)
                .min_by_key(|(_, target)| target.last_used_tick)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(old) = self.targets.remove(&victim) {
                self.bytes_total = self.bytes_total.saturating_sub(old.bytes);
            }
        }
        Ok(())
    }

    /// Begin the offscreen scene pass for `id` (clearing color to `clear`
    /// and depth to 1.0). The caller issues its depth-tested draws inside
    /// the returned pass, then ends it; [`blit`](Self::blit) composites in
    /// `paint`. Returns `false` when `id` has no target (call [`ensure`](Self::ensure) first).
    ///
    /// The depth and stencil attachments are discarded when the pass ends:
    /// [`blit`](Self::blit) only samples the color target, so nothing in
    /// this crate reads them afterwards. A caller that needs the depth
    /// buffer afterwards must keep its own copy.
    ///
    /// `clear` uses straight (un-premultiplied) alpha, which the blit
    /// premultiplies before compositing.
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
                // Nothing reads the offscreen depth/stencil after this pass:
                // the blit declares `depth_write_enabled: false` /
                // `depth_compare: Always`.
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Discard,
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
        self.targets.get(id).map(|_| DEPTH_FORMAT)
    }

    /// Composite the offscreen scene for `id` into the main pass. The
    /// renderer has already set the viewport to the callback rect, which
    /// matches the offscreen texture 1:1 (both come from the painted frame
    /// geometry). Composites over the UI already drawn this frame, so a
    /// transparent scene keeps it visible. No-op when `id` has no target.
    pub fn blit<P: BlitRenderPass>(&self, id: &str, rpass: &mut P) {
        let Some(t) = self.targets.get(id) else {
            return;
        };
        let Some(pipeline) = self.pipelines.get(&(t.key.0, t.key.1)) else {
            log::warn!("depth composite {id}: blit pipeline missing");
            return;
        };
        rpass.set_blit_pipeline(pipeline);
        rpass.set_blit_bind_group(0, &t.blit_bind, &[]);
        rpass.draw_blit_triangle();
    }

    pub fn blit_callback(&self, id: &str, rpass: &mut CallbackRenderPass<'_, '_>) {
        self.blit(id, rpass);
    }
}
