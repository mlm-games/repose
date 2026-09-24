use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
#[cfg(feature = "winit-surface")]
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Weak};

use repose_core::color::{ChromaSiting, ColorInfo, PixelFormat};
use repose_core::{Brush, FontStyle, Scene, SceneNode, StrokeCap, Transform, Vec2};
#[cfg(feature = "winit-surface")]
use repose_core::{GlyphRasterConfig, PresentModePref, RenderBackend, request_frame};
#[cfg(feature = "winit-surface")]
use wgpu::Instance;

mod slug;

fn align_up(value: u64, alignment: u64) -> anyhow::Result<u64> {
    if !alignment.is_power_of_two() {
        anyhow::bail!("alignment must be a power of two");
    }
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .ok_or_else(|| anyhow::anyhow!("alignment overflow"))
}

mod commands;
pub use commands::apply_render_commands;

pub mod offscreen;

mod callback;
pub use callback::{
    Callback, CallbackRenderPass, CallbackResources, ScreenDescriptor, WgpuCallback,
};

mod depth_composite;
pub use depth_composite::{BlitRenderPass, DepthComposite};

#[derive(Clone)]
struct UploadRing {
    buf: wgpu::Buffer,
    cap: u64,
    head: u64,
    usage: wgpu::BufferUsages,
}

impl UploadRing {
    fn new(device: &wgpu::Device, label: &str, cap: u64, usage: wgpu::BufferUsages) -> Self {
        let max = device.limits().max_buffer_size;
        let cap = cap.max(4).min(max).min(MAX_UPLOAD_RING_BYTES).max(4);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage,
            mapped_at_creation: false,
        });
        Self {
            buf,
            cap,
            head: 0,
            usage,
        }
    }

    fn reset(&mut self) {
        self.head = 0;
    }

    fn grow_to_fit(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        needed: u64,
    ) -> anyhow::Result<()> {
        if needed == 0 {
            return Ok(());
        }
        let start = align_up(self.head, 4)?;
        let end = start
            .checked_add(needed)
            .ok_or_else(|| anyhow::anyhow!("upload ring offset overflow"))?;
        if end <= self.cap {
            return Ok(());
        }
        let required = align_up(end, 4)?;
        let doubled = self.cap.checked_mul(2).unwrap_or(0);
        let new_cap = required.max(doubled).max(256);
        let new_cap = align_up(new_cap, 4)?;
        if new_cap > device.limits().max_buffer_size || new_cap > MAX_UPLOAD_RING_BYTES {
            anyhow::bail!("upload ring exceeds resource budget");
        }
        if !self.usage.contains(wgpu::BufferUsages::COPY_SRC) {
            anyhow::bail!("upload ring growth requires COPY_SRC");
        }
        let old = self.buf.clone();
        self.buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("upload ring (grown)"),
            size: new_cap,
            usage: self.usage,
            mapped_at_creation: false,
        });
        self.cap = new_cap;
        if self.head > 0 {
            let copy_size = align_up(self.head, 4)?;
            if copy_size > old.size() {
                anyhow::bail!("upload ring old data exceeds old buffer");
            }
            encoder.copy_buffer_to_buffer(&old, 0, &self.buf, 0, copy_size);
        }
        Ok(())
    }

    fn alloc_write(&mut self, queue: &wgpu::Queue, bytes: &[u8]) -> anyhow::Result<u64> {
        let len = u64::try_from(bytes.len())
            .map_err(|_| anyhow::anyhow!("upload byte length exceeds u64"))?;
        let start = align_up(self.head, 4)?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("upload ring offset overflow"))?;
        if end > self.cap {
            anyhow::bail!(
                "upload ring overflow: start={start} len={len} cap={}",
                self.cap
            );
        }
        queue.write_buffer(&self.buf, start, bytes);
        self.head = end;
        Ok(start)
    }
}

struct InstancedPipe<I: bytemuck::Pod> {
    ring: UploadRing,
    _marker: std::marker::PhantomData<I>,
}

impl<I: bytemuck::Pod> InstancedPipe<I> {
    fn new(ring: UploadRing) -> Self {
        Self {
            ring,
            _marker: std::marker::PhantomData,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        data: &[I],
    ) -> Option<(u64, u32)> {
        if data.is_empty() {
            return None;
        }
        let count = u32::try_from(data.len()).ok()?;
        let bytes = bytemuck::cast_slice(data);
        let byte_len = u64::try_from(bytes.len()).ok()?;
        if byte_len == 0 || byte_len > device.limits().max_buffer_size {
            return None;
        }
        self.ring
            .grow_to_fit(device, encoder, byte_len)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        let off = self
            .ring
            .alloc_write(queue, bytes)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        Some((off, count))
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

fn make_globals(target_w: f32, target_h: f32) -> Globals {
    Globals {
        ndc_to_px: [target_w * 0.5, target_h * 0.5],
        _pad: [0.0, 0.0],
    }
}

pub struct WgpuSceneRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub output_format: wgpu::TextureFormat,
    pub output_width: u32,
    pub output_height: u32,
    /// Pixels per point (DPI scale) for `ScreenDescriptor` / `PaintCallbackInfo`.
    pub pixels_per_point: f32,

    // Render pipelines. Two sets: one for the MSAA surface pass, one for
    // graphics-layer render-to-texture passes (sample_count = 1).
    surface_pipes: Pipelines,
    working_space_pipes: Pipelines,
    layer_pipes: Pipelines,
    working_space_layer_pipes: Pipelines,

    // Instanced draw rings
    rects: InstancedPipe<RectInstance>,
    borders: InstancedPipe<BorderInstance>,
    ellipses: InstancedPipe<EllipseInstance>,
    ellipse_borders: InstancedPipe<EllipseBorderInstance>,
    arcs: InstancedPipe<ArcInstance>,
    glyph_mask: InstancedPipe<GlyphInstance>,
    glyph_color: InstancedPipe<GlyphInstance>,

    // Image bind layouts and shared sampler
    image_bind_layout_rgba: wgpu::BindGroupLayout,
    image_bind_layout_nv12: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    layer_sampler: wgpu::Sampler,
    layer_sampler_linear: wgpu::Sampler,

    // Blur composite ring (for graphics-layer drop shadows)
    blur_ring: UploadRing,

    text_bind_layout: wgpu::BindGroupLayout,

    // Stencil clip ring
    clip_ring: UploadRing,

    // Projective layer-composite ring (one ProjectiveInstance per flattened
    // perspective subtree)
    projective_ring: UploadRing,

    // Backdrop-blend composite ring (one BlendInstance per isolated blend)
    blend_ring: UploadRing,

    // Tessellated vector glyph pipeline (always enabled)
    slug_enabled: bool,
    slug_ring: UploadRing,
    slug_cache: slug::GlyphSlugCache,

    // Instanced NV12 ring
    nv12: InstancedPipe<Nv12Instance>,

    // Tessellated vector mesh rendering (host-provided, e.g. lyon output).
    mesh_verts: UploadRing,
    mesh_indices: UploadRing,
    mesh_uniform_buf: wgpu::Buffer,
    mesh_bind: wgpu::BindGroup,
    mesh_uniform_head: u64,
    mesh_uniform_alignment: u64,
    mesh_uniform_cap: u64,

    /// Translator-owned flatten layer ids used by the previous frame;
    /// drained from the layer pool at the start of each translation (they
    /// are single-frame by construction).
    flatten_layer_ids: Vec<u32>,

    /// Backdrop snapshots keyed by isolated-blend layer id. Filled during
    /// translation (texture allocated) and populated by a texture copy at
    /// execution time, before the blend composite draws.
    blend_snapshots: std::collections::HashMap<u32, BlendSnapshot>,
    /// (blend layer id, parent target) copies to run before the pass that
    /// composites the blend. Executed between passes: copies the current
    /// target region into the snapshot texture.
    blend_copies: Vec<BlendCopy>,

    msaa_samples: u32,
    working_space_msaa_samples: u32,

    // Depth-stencil target
    depth_stencil_tex: wgpu::Texture,
    depth_stencil_view: wgpu::TextureView,

    // Optional MSAA color target
    msaa_tex: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    ws_msaa_tex: Option<wgpu::Texture>,
    ws_msaa_view: Option<wgpu::TextureView>,
    surface_resolve_tex: Option<wgpu::Texture>,
    surface_resolve_view: Option<wgpu::TextureView>,
    surface_resolve_bytes: u64,
    msaa_bytes: u64,
    ws_msaa_bytes: u64,
    depth_stencil_bytes: u64,
    working_space_bytes: u64,
    gpu_budget_bytes: u64,

    globals_buf: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,

    // Glyph atlas
    atlas_mask: AtlasA8,
    atlas_color: AtlasRGBA,

    // Image management
    next_image_handle: u64,
    images: HashMap<u64, ImageTex>,
    retained: HashMap<u64, RetainedImage>,
    retained_bytes_total: u64,

    // A8 coverage-tile management (host-rasterized masks composited tinted;
    // no retained CPU copies — tiles are immutable and re-registered).
    next_coverage_handle: u64,
    coverages: HashMap<u64, CoverageTex>,

    // Eviction stats
    frame_index: u64,
    image_bytes_total: u64,
    image_evict_after_frames: u64,
    image_budget_bytes: u64,

    // Graphics layer pool. Maps `SceneNode::BeginLayer::layer_id` to a
    // cached offscreen render target.
    layer_pool: HashMap<u32, LayerTarget>,
    producer_layer_ids: Vec<u32>,
    layer_bytes_total: u64,

    // Linear working-space mode (default off -> fast playback path).
    // When enabled, the scene is rendered into an Rgba16Float intermediate
    // texture, then a final full-screen pass applies the display OETF.
    working_space: bool,
    ws_tex: Option<wgpu::Texture>,
    ws_view: Option<wgpu::TextureView>,
    ws_bind: Option<wgpu::BindGroup>,
    display_pipeline: Option<wgpu::RenderPipeline>,
    display_layout: Option<wgpu::BindGroupLayout>,

    pub callback_resources: CallbackResources,
    callback_scoped_resources: HashMap<CallbackScopeKey, CallbackResources>,
    callback_scope_uses: HashMap<CallbackScopeKey, CallbackScopeUse>,
    callback_scope_payloads: HashMap<CallbackScopeKey, Weak<Callback>>,
    callback_scope_clock: u64,
    blend_snapshot_bytes_total: u64,
    frame_active: bool,
    last_render_error: Option<String>,
}

pub struct WgpuSurfaceBackend {
    #[cfg(feature = "winit-surface")]
    instance: Option<wgpu::Instance>,
    #[cfg(feature = "winit-surface")]
    window: Option<Arc<winit::window::Window>>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub surface_config: Option<wgpu::SurfaceConfiguration>,
    #[cfg_attr(not(feature = "winit-surface"), allow(dead_code))]
    pending_reconfigure: bool,
    pub renderer: WgpuSceneRenderer,
}

impl std::ops::Deref for WgpuSurfaceBackend {
    type Target = WgpuSceneRenderer;
    fn deref(&self) -> &Self::Target {
        &self.renderer
    }
}
impl std::ops::DerefMut for WgpuSurfaceBackend {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.renderer
    }
}

#[cfg(feature = "winit-surface")]
pub type WgpuBackend = WgpuSurfaceBackend;

impl Drop for WgpuSceneRenderer {
    fn drop(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_millis(100)),
            });
        }
    }
}

#[derive(Clone)]
struct LayerTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind: wgpu::BindGroup,
    bind_linear: wgpu::BindGroup,
    depth_stencil_view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    bytes: u64,
    rect_px: (f32, f32, f32, f32),
}

/// Backdrop snapshot for one isolated blend: a copy of the current target
/// region taken before the source layer is composited, sampled as the
/// "backdrop" input of the blend shader.
#[derive(Clone)]
struct BlendSnapshot {
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
    width: u32,
    height: u32,
    bytes: u64,
}

struct BlendCopy {
    pass_id: u64,
    blend_id: u32,
    target: PassTarget,
    region: repose_core::Rect,
}

#[derive(Clone)]
enum ActiveClip {
    Rect {
        off: u64,
        cnt: u32,
        rect: repose_core::Rect,
        radii: [f32; 4],
        difference: bool,
        applied: bool,
        blocked: bool,
    },
    Vector {
        voff: u64,
        vcnt: u32,
        ioff: u64,
        icnt: u32,
        uoff: u64,
        mesh: Arc<repose_core::VectorMeshData>,
        affine: [f32; 6],
        difference: bool,
        applied: bool,
        blocked: bool,
    },
}

impl ActiveClip {
    fn difference(&self) -> bool {
        match self {
            Self::Rect { difference, .. } | Self::Vector { difference, .. } => *difference,
        }
    }

    fn blocked(&self) -> bool {
        match self {
            Self::Rect { blocked, .. } | Self::Vector { blocked, .. } => *blocked,
        }
    }
}

/// Identifies which render target a `Pass` draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PassTarget {
    Surface,
    Layer(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CallbackScopeKey {
    callback: usize,
    target: PassTarget,
    width: u32,
    height: u32,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
    pixels_per_point_bits: u32,
}

#[derive(Clone, Copy)]
struct CallbackScopeUse {
    tick: u64,
    frame: u64,
}

fn callback_scope_key(
    payload: &repose_core::PaintCallbackPayload,
    target: PassTarget,
    descriptor: &ScreenDescriptor,
) -> CallbackScopeKey {
    CallbackScopeKey {
        callback: Arc::as_ptr(payload) as *const () as usize,
        target,
        width: descriptor.size_in_pixels[0],
        height: descriptor.size_in_pixels[1],
        target_format: descriptor.target_format,
        sample_count: descriptor.sample_count,
        pixels_per_point_bits: descriptor.pixels_per_point.to_bits(),
    }
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
    arcs: wgpu::RenderPipeline,
    text_mask: wgpu::RenderPipeline,
    text_color: wgpu::RenderPipeline,
    image_rgba: wgpu::RenderPipeline,
    /// Tinted A8 coverage composite (`coverage.wgsl`): same vertex
    /// attributes and bind groups as the text/color path, sampling a
    /// single-channel tile registered with `register_coverage_a8`.
    coverage: wgpu::RenderPipeline,
    image_nv12: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    blur_content: wgpu::RenderPipeline,
    clip_bin: wgpu::RenderPipeline,
    clip_dec: wgpu::RenderPipeline,
    slug: Option<wgpu::RenderPipeline>,
    /// Tessellated vector mesh (fill/stroke). Uses an `Equal` stencil compare
    /// so world content is correctly masked to the active `PushVectorClip`
    /// shape (outside any clip the stencil is 0 == ref 0, so it draws).
    mesh: wgpu::RenderPipeline,
    /// Screen-space overlay meshes: `LessEqual` compare so they always draw
    /// regardless of any active vector clip.
    mesh_overlay: wgpu::RenderPipeline,
    /// Fixed-function blend variants of the mesh pipeline, selected by
    /// `BlendMode`. Backdrop-dependent modes (overlay, color-dodge/burn,
    /// hard/soft-light, exclusion, hue/saturation/color/luminosity) use the
    /// `blend_layer` shader instead.
    mesh_add: wgpu::RenderPipeline,
    mesh_multiply: wgpu::RenderPipeline,
    mesh_screen: wgpu::RenderPipeline,
    mesh_darken: wgpu::RenderPipeline,
    mesh_lighten: wgpu::RenderPipeline,
    /// Backdrop-blend composite: samples an isolated source layer and the
    /// current target, applies the CSS blend formula selected by a uniform,
    /// and composites premultiplied source-over.
    blend_layer: wgpu::RenderPipeline,
    /// Stencil increment for vector clips.
    mesh_clip_inc: wgpu::RenderPipeline,
    /// Stencil decrement for vector clips.
    mesh_clip_dec: wgpu::RenderPipeline,
    /// Projective layer composite (perspective flattening): samples a
    /// graphics-layer texture through a 2D projective map. Drawn with a
    /// `ProjectiveInstance` from `projective_ring`.
    projective_layer: wgpu::RenderPipeline,
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
        stencil_for_clip_dec: &wgpu::DepthStencilState,
        clip_vertex_layout: &wgpu::VertexBufferLayout,
        mesh_bind_layout: &wgpu::BindGroupLayout,
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
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(concat!(
                        "shaders/", $shader, ".wgsl"
                    )))),
                });
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<$inst_type>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: $attrs,
                        })],
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
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 3,
                offset: 36,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 4,
                offset: 48,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 5,
                offset: 64,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 6,
                offset: 80,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 7,
                offset: 88,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 8,
                offset: 96,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 9,
                offset: 112,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let border_attrs: &[wgpu::VertexAttribute] = &[
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
                format: wgpu::VertexFormat::Float32,
            },
            wgpu::VertexAttribute {
                shader_location: 3,
                offset: 36,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 4,
                offset: 48,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 5,
                offset: 52,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 6,
                offset: 68,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 7,
                offset: 84,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 8,
                offset: 92,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 9,
                offset: 100,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 10,
                offset: 116,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let ellipse_attrs: &[wgpu::VertexAttribute] = &[
            wgpu::VertexAttribute {
                shader_location: 0,
                offset: 0,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 1,
                offset: 16,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 2,
                offset: 20,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 3,
                offset: 32,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 4,
                offset: 48,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 5,
                offset: 64,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 6,
                offset: 72,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 7,
                offset: 80,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 8,
                offset: 96,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];
        let ellipse_border_attrs: &[wgpu::VertexAttribute] = &[
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
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 4,
                offset: 28,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 5,
                offset: 32,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 6,
                offset: 48,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 7,
                offset: 64,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 8,
                offset: 72,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 9,
                offset: 80,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 10,
                offset: 96,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];

        make_content_pipeline!(rects, "rect", RectInstance, rect_attrs);
        make_content_pipeline!(borders, "border", BorderInstance, border_attrs);
        make_content_pipeline!(ellipses, "ellipse", EllipseInstance, ellipse_attrs);
        make_content_pipeline!(
            ellipse_borders,
            "ellipse_border",
            EllipseBorderInstance,
            ellipse_border_attrs
        );

        let arc_attrs: &[wgpu::VertexAttribute] = &[
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
                format: wgpu::VertexFormat::Float32,
            },
            wgpu::VertexAttribute {
                shader_location: 4,
                offset: 28,
                format: wgpu::VertexFormat::Float32,
            },
            wgpu::VertexAttribute {
                shader_location: 5,
                offset: 32,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 6,
                offset: 36,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 7,
                offset: 48,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 8,
                offset: 64,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                shader_location: 9,
                offset: 80,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 10,
                offset: 88,
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                shader_location: 11,
                offset: 96,
                format: wgpu::VertexFormat::Uint32,
            },
            wgpu::VertexAttribute {
                shader_location: 12,
                offset: 100,
                format: wgpu::VertexFormat::Float32,
            },
            wgpu::VertexAttribute {
                shader_location: 13,
                offset: 112,
                format: wgpu::VertexFormat::Float32x4,
            },
        ];

        make_content_pipeline!(arcs, "arc", ArcInstance, arc_attrs);

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
                wgpu::VertexAttribute {
                    shader_location: 3,
                    offset: 48,
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
                buffers: &[Some(glyph_vertex.clone())],
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
                buffers: &[Some(glyph_vertex.clone())],
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

        // Tinted A8 coverage composite. Same vertex attributes (GlyphInstance)
        // and bind groups (globals + texture/sampler) as the text color path,
        // sampling R8 tiles uploaded via `register_coverage_a8`.
        let coverage_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("coverage.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/coverage.wgsl"))),
        });
        let coverage = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("coverage pipeline (tinted a8)"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &coverage_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(glyph_vertex.clone())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &coverage_shader,
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
                buffers: &[Some(wgpu::VertexBufferLayout {
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
                        wgpu::VertexAttribute {
                            shader_location: 4,
                            offset: 56,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 5,
                            offset: 72,
                            format: wgpu::VertexFormat::Uint32,
                        },
                    ],
                })],
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

        // Content blur pipeline (full RGBA gaussian blur)
        let blur_content_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur_content.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/blur_content.wgsl"
            ))),
        });
        let blur_content = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur content pipeline"),
            layout: Some(&blur_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blur_content_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
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
                        wgpu::VertexAttribute {
                            shader_location: 4,
                            offset: 56,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 5,
                            offset: 72,
                            format: wgpu::VertexFormat::Uint32,
                        },
                    ],
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blur_content_shader,
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
                buffers: &[Some(wgpu::VertexBufferLayout {
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
                        wgpu::VertexAttribute {
                            shader_location: 4,
                            offset: 52,
                            format: wgpu::VertexFormat::Float32,
                        },
                        wgpu::VertexAttribute {
                            shader_location: 5,
                            offset: 56,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                })],
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

        let clip_color_target = wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::empty(),
        };

        // Clipping
        let clip_shader_bin = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("clip_round_rect_bin.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/clip_round_rect_bin.wgsl"
            ))),
        });
        let clip_bin = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (bin)"),
            layout: Some(clip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clip_shader_bin,
                entry_point: Some("vs_main"),
                buffers: &[Some(clip_vertex_layout.clone())],
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
        let clip_dec = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("clip pipeline (dec)"),
            layout: Some(clip_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &clip_shader_bin,
                entry_point: Some("vs_main"),
                buffers: &[Some(clip_vertex_layout.clone())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &clip_shader_bin,
                entry_point: Some("fs_main"),
                targets: &[Some(clip_color_target.clone())],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(stencil_for_clip_dec.clone()),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let slug = Some(slug::create_pipeline(
            device,
            format,
            sample_count,
            stencil_for_content,
        ));

        // Tessellated vector mesh pipeline (host-supplied vertex/index data).
        let mesh_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/mesh.wgsl"))),
        });
        let mesh_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[Some(globals_layout), Some(mesh_bind_layout)],
            immediate_size: 0,
        });
        let mesh_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    shader_location: 0,
                    offset: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 1,
                    offset: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 2,
                    offset: 24,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        };
        let make_mesh_pipeline =
            |label: &str, depth: &wgpu::DepthStencilState, color: &wgpu::ColorTargetState| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&mesh_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &mesh_shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Some(mesh_vertex_layout.clone())],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &mesh_shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(color.clone())],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: Some(depth.clone()),
                    multisample: msaa_state,
                    multiview_mask: None,
                    cache: None,
                })
            };
        let mesh_color_target = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let mut stencil_for_mesh = stencil_for_content.clone();
        stencil_for_mesh.stencil.front.compare = wgpu::CompareFunction::Equal;
        stencil_for_mesh.stencil.back.compare = wgpu::CompareFunction::Equal;
        let mut stencil_for_overlay = stencil_for_content.clone();
        stencil_for_overlay.stencil.front.compare = wgpu::CompareFunction::LessEqual;
        stencil_for_overlay.stencil.back.compare = wgpu::CompareFunction::LessEqual;
        let mesh = make_mesh_pipeline("mesh pipeline", &stencil_for_mesh, &mesh_color_target);
        let mesh_overlay = make_mesh_pipeline(
            "mesh overlay pipeline",
            &stencil_for_overlay,
            &mesh_color_target,
        );
        let mesh_clip_inc = make_mesh_pipeline(
            "mesh clip (inc) pipeline",
            stencil_for_clip_inc,
            &clip_color_target,
        );
        let mesh_clip_dec = make_mesh_pipeline(
            "mesh clip (dec) pipeline",
            stencil_for_clip_dec,
            &clip_color_target,
        );
        let mesh_blend_target = |blend: wgpu::BlendState| wgpu::ColorTargetState {
            format,
            blend: Some(blend),
            write_mask: wgpu::ColorWrites::ALL,
        };
        let premult_alpha = wgpu::BlendComponent::OVER;
        let mesh_add = make_mesh_pipeline(
            "mesh pipeline (add)",
            &stencil_for_mesh,
            &mesh_blend_target(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
        );
        let mesh_multiply = make_mesh_pipeline(
            "mesh pipeline (multiply)",
            &stencil_for_mesh,
            &mesh_blend_target(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: premult_alpha,
            }),
        );
        let mesh_screen = make_mesh_pipeline(
            "mesh pipeline (screen)",
            &stencil_for_mesh,
            &mesh_blend_target(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrc,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: premult_alpha,
            }),
        );
        let mesh_darken = make_mesh_pipeline(
            "mesh pipeline (darken)",
            &stencil_for_mesh,
            &mesh_blend_target(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Min,
                },
                alpha: premult_alpha,
            }),
        );
        let mesh_lighten = make_mesh_pipeline(
            "mesh pipeline (lighten)",
            &stencil_for_mesh,
            &mesh_blend_target(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Max,
                },
                alpha: premult_alpha,
            }),
        );
        // Projective layer composite (perspective flattening). Same
        // bind groups as the text/image path (globals + layer texture), with
        // per-instance projected corners. Like `image_rgba` it draws into the
        // parent target, so it shares the content stencil state.
        let projective_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("projective_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/projective_layer.wgsl"
            ))),
        });
        let projective_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("projective layer pipeline layout"),
                bind_group_layouts: &[Some(globals_layout), Some(text_bind_layout)],
                immediate_size: 0,
            });
        let projective_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ProjectiveInstance>() as u64,
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
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 3,
                    offset: 24,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    shader_location: 4,
                    offset: 32,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 5,
                    offset: 48,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 6,
                    offset: 64,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };
        let projective_layer = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("projective layer composite pipeline"),
            layout: Some(&projective_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &projective_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(projective_vertex_layout)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &projective_shader,
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

        let blend_layer_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blend_layer.wgsl"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(
                "shaders/blend_layer.wgsl"
            ))),
        });
        let blend_layer_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blend layer pipeline layout"),
            bind_group_layouts: &[
                Some(globals_layout),
                Some(text_bind_layout),
                Some(text_bind_layout),
            ],
            immediate_size: 0,
        });
        let blend_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BlendInstance>() as u64,
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
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 4,
                    offset: 64,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        };
        let blend_layer = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blend layer pipeline"),
            layout: Some(&blend_layer_layout),
            vertex: wgpu::VertexState {
                module: &blend_layer_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(blend_vertex_layout)],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blend_layer_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
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

        Self {
            rects,
            borders,
            ellipses,
            ellipse_borders,
            arcs,
            text_mask,
            text_color,
            image_rgba,
            image_nv12,
            coverage,
            blur,
            blur_content,
            clip_bin,
            clip_dec,
            slug,
            mesh,
            mesh_add,
            mesh_multiply,
            mesh_screen,
            mesh_darken,
            mesh_lighten,
            blend_layer,
            mesh_overlay,
            mesh_clip_inc,
            mesh_clip_dec,
            projective_layer,
        }
    }
}

/// A segment of the frame that draws into a single render target.
struct Pass {
    id: u64,
    target: PassTarget,
    /// The scissor to apply when the pass is opened.
    initial_scissor: (u32, u32, u32, u32),
    /// The currently active scissor in the translated pass. `None` means an
    /// empty intersection and all draws must be suppressed.
    active_scissor: Option<(u32, u32, u32, u32)>,
    /// `None` means `LoadOp::Load` (resume existing content);
    /// `Some(c)` means `LoadOp::Clear(c)`.
    clear_color: Option<[f32; 4]>,
    cmds: Vec<Cmd>,
}

struct LayerState {
    layer_id: u32,
    parent_target: PassTarget,
    parent_scissors: Vec<repose_core::Rect>,
    parent_root_clip: repose_core::Rect,
    parent_size: (f32, f32),
    parent_transform: Transform,
    parent_scissor: Option<(u32, u32, u32, u32)>,
    parent_clips: Vec<ActiveClip>,
    alpha: f32,
    blur: (f32, f32),
    rectangle_edge: bool,
}

/// One translator-flattened perspective layer (see
/// `push_perspective_layer`): the projective map and everything needed to
/// restore the parent target on the matching pop.
struct FlattenRecord {
    /// `transform_stack.len()` before this flatten pushed its two entries
    /// (stripped affine + layer-local shift).
    stack_len: usize,
    layer_id: u32,
    /// Full projective map (row-major 3x3): affine ancestors over the
    /// perspective node, in parent-target coordinates.
    map: [f32; 9],
    /// Layer rect in the parent target's coordinates.
    layer_rect: repose_core::Rect,
    saved_scissor_stack: Vec<repose_core::Rect>,
    saved_root: repose_core::Rect,
    saved_size: (f32, f32),
    saved_active_scissor: Option<(u32, u32, u32, u32)>,
    saved_clips: Vec<ActiveClip>,
}

/// First translator-owned flatten layer id. Producer ids start at 1 per
/// scene, so this range never collides; ids are drained from the layer pool
/// after each frame, so reuse across frames is safe.
const FLATTEN_ID_BASE: u32 = 0xF000_0000;
const MAX_GRAPHICS_LAYERS: usize = 128;
const MAX_GRAPHICS_LAYER_BYTES: u64 = 256 * 1024 * 1024;
const STENCIL_BASE: u32 = 128;
const STENCIL_MAX_DEPTH: u32 = 127;

#[allow(non_snake_case)]
enum Cmd {
    ClipPush {
        off: u64,
        cnt: u32,
        scissor: (u32, u32, u32, u32),
        difference: bool,
        applied: bool,
    },
    ClipPop {
        off: u64,
        cnt: u32,
        scissor: (u32, u32, u32, u32),
        difference: bool,
        applied: bool,
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
    Arc {
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
    GlyphsVector {
        off: u64,
        cnt: u32,
    },
    ImageRgba {
        off: u64,
        cnt: u32,
        handle: u64,
    },
    /// Composite a tinted A8 coverage tile (`SceneNode::Coverage`). The
    /// instance lives in `self.glyph_color.ring` (a `GlyphInstance`); the
    /// bind comes from the coverage registry.
    Coverage {
        off: u64,
        cnt: u32,
        handle: u64,
    },
    ImageNv12 {
        off: u64,
        cnt: u32,
        handle: u64,
    },
    /// Composite a previously-rendered graphics layer back into the
    /// current target as a textured quad. The quad's vertex buffer
    /// lives in `self.glyph_color.ring` (a `GlyphInstance`).
    CompositeLayer {
        off: u64,
        cnt: u32,
        layer_id: u32,
    },
    /// Composite a blurred drop shadow of a previously-rendered graphics
    /// layer. The quad's vertex buffer lives in `self.blur_ring` (a
    /// `BlurInstance`).
    CompositeShadow {
        off: u64,
        cnt: u32,
        layer_id: u32,
    },
    /// Apply gaussian blur to a layer and composite the blurred result.
    /// Uses the `blur_content` pipeline (full RGBA blur).
    CompositeBlur {
        off: u64,
        cnt: u32,
        layer_id: u32,
    },
    /// Composite a flattened perspective layer through its projective map.
    /// The instance lives in `self.projective_ring` (a `ProjectiveInstance`
    /// with CPU-projected NDC corners); sampled from the layer's texture
    /// with perspective-correct UVs by the `projective_layer` pipeline.
    CompositeProjective {
        off: u64,
        cnt: u32,
        layer_id: u32,
    },
    /// Draw a tessellated vector mesh (solid or gradient paint).
    VectorMesh {
        voff: u64,
        vcnt: u32,
        ioff: u64,
        icnt: u32,
        uoff: u64,
        blend: repose_core::BlendMode,
    },
    /// Composite an isolated source layer over the current target with a
    /// backdrop-dependent CSS blend mode. The shader samples both textures
    /// and composites source-over by hand, so the pass runs REPLACE.
    /// `parent` records which target the composite draws into (surface or
    /// layer); the instance NDC is mapped against it, so any parent origin
    /// works.
    BlendLayer {
        off: u64,
        cnt: u32,
        src_layer: u32,
        dst_layer: Option<u32>,
        parent: PassTarget,
        scissor: (u32, u32, u32, u32),
    },
    /// Draw a screen-space overlay mesh (identity transform, device pixels).
    VectorOverlay {
        voff: u64,
        vcnt: u32,
        ioff: u64,
        icnt: u32,
        uoff: u64,
    },
    /// Increment the stencil buffer with a tessellated vector mask.
    /// `difference` marks an inverse (`\iclip`-style) mask: content draws
    /// *outside* it. The counting still balances (push increments, pop
    /// decrements); only the depth bookkeeping differs (see executor).
    VectorClipPush {
        voff: u64,
        vcnt: u32,
        ioff: u64,
        icnt: u32,
        uoff: u64,
        scissor: (u32, u32, u32, u32),
        difference: bool,
        applied: bool,
    },
    /// Decrement the stencil buffer with the matching vector mask.
    VectorClipPop {
        voff: u64,
        vcnt: u32,
        ioff: u64,
        icnt: u32,
        uoff: u64,
        scissor: (u32, u32, u32, u32),
        difference: bool,
        applied: bool,
    },
    Callback {
        rect: repose_core::Rect,
        clip_rect: repose_core::Rect,
        scissor: (u32, u32, u32, u32),
        restore_scissor: (u32, u32, u32, u32),
        callback_id: usize,
        payload: repose_core::PaintCallbackPayload,
    },
}

/// A registered A8 coverage tile: single-channel mask sampled as coverage
/// by `SceneNode::Coverage`. Tiles are immutable; producers re-register on
/// geometry change and `remove_coverage` stale handles (unused tiles also
/// age out via the image eviction policy).
struct CoverageTex {
    // Held to keep the GPU texture alive (freed on remove/evict).
    #[allow(dead_code)]
    tex: wgpu::Texture,
    bind: wgpu::BindGroup,
    w: u32,
    h: u32,
    last_used_frame: u64,
    bytes: u64,
}

enum ImageTex {
    Rgba {
        tex: wgpu::Texture,
        bind: wgpu::BindGroup,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
        last_used_frame: u64,
        bytes: u64,
    },
    /// For a user-provided texture view.
    User {
        bind: wgpu::BindGroup,
        w: u32,
        h: u32,
        last_used_frame: u64,
        bytes: u64,
    },
    Nv12 {
        tex_y: wgpu::Texture,
        tex_uv: wgpu::Texture,
        bind: wgpu::BindGroup,
        yuv_buf: wgpu::Buffer,
        w: u32,
        h: u32,
        pixel_format: PixelFormat,
        fourcc: Option<u32>,
        color_info: ColorInfo,
        external: bool,
        last_used_frame: u64,
        bytes: u64,
    },
}

impl ImageTex {
    fn evictable(&self) -> bool {
        match self {
            Self::Rgba { .. } => true,
            Self::User { .. } => false,
            Self::Nv12 { external, .. } => !*external,
        }
    }
}

#[derive(Clone)]
struct RetainedImage {
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    rgba: Vec<u8>,
    last_used_frame: u64,
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
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RectInstance {
    xywh: [f32; 4],
    radii: [f32; 4],
    brush_type: u32,
    grad_kind: u32,
    _pad: [f32; 2],
    color0: [f32; 4],
    color1: [f32; 4],
    grad_p0: [f32; 2],
    grad_p1: [f32; 2],
    tile_mode: u32,
    _pad2: [f32; 3],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BorderInstance {
    xywh: [f32; 4],
    radii: [f32; 4],
    stroke: f32,
    brush_type: u32,
    _pad: [f32; 2],
    grad_kind: u32,
    color0: [f32; 4],
    color1: [f32; 4],
    grad_p0: [f32; 2],
    grad_p1: [f32; 2],
    tile_mode: u32,
    _pad2: [f32; 3],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EllipseInstance {
    xywh: [f32; 4],
    brush_type: u32,
    grad_kind: u32,
    _pad: [f32; 2],
    color0: [f32; 4],
    color1: [f32; 4],
    grad_p0: [f32; 2],
    grad_p1: [f32; 2],
    tile_mode: u32,
    _pad2: [f32; 3],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EllipseBorderInstance {
    xywh: [f32; 4],
    stroke: f32,
    pad: f32,
    brush_type: u32,
    grad_kind: u32,
    color0: [f32; 4],
    color1: [f32; 4],
    grad_p0: [f32; 2],
    grad_p1: [f32; 2],
    tile_mode: u32,
    _pad2: [f32; 3],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ArcInstance {
    xywh: [f32; 4],
    start_angle: f32,
    sweep_angle: f32,
    stroke: f32,
    pad: f32,
    brush_type: u32,
    grad_kind: u32,
    _pad0: [f32; 2],
    color0: [f32; 4],
    color1: [f32; 4],
    grad_p0: [f32; 2],
    grad_p1: [f32; 2],
    tile_mode: u32,
    cap: f32, // 0=Butt, 1=Round, 2=Square
    _pad1: [f32; 2],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlyphInstance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurInstance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    blur_uv: [f32; 2],
    fwd_mat: [f32; 4],
    edge_mode: u32,
    _pad: [f32; 3],
}

/// Projective layer-composite instance: the four layer-rect corners projected
/// to NDC (`c0..c3`, counter-clockwise from top-left) with their homogeneous
/// `w`, the layer-texture uv bounds, and a group alpha. Matches
/// `projective_layer.wgsl` (offsets: c0@0 c1@8 c2@16 c3@24 uv@32 w@48
/// alpha@64; stride 80).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ProjectiveInstance {
    c0: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    c3: [f32; 2],
    uv: [f32; 4],
    w: [f32; 4],
    alpha: f32,
    _pad: [f32; 3],
}

/// CPU-computed Y′CbCr -> R′G′B′ transform uploaded as a uniform buffer.
/// Layout matches the WGSL `YuvTransform` struct (5 × vec4<f32>).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct YuvTransformRaw {
    row0: [f32; 4],
    row1: [f32; 4],
    row2: [f32; 4],
    b: [f32; 4],
    transfer: [f32; 4],
}

fn make_yuv_transform_raw(color_info: ColorInfo, pixel_format: PixelFormat) -> YuvTransformRaw {
    let yuv = color_info.to_yuv_transform();
    let sample_scale = match pixel_format {
        PixelFormat::P010 => 65535.0 / (64.0 * 1020.0),
        _ => 1.0,
    };
    let transfer_mode = match color_info.transfer {
        repose_core::color::Transfer::Srgb => 0.0,
        repose_core::color::Transfer::Bt709 => 1.0,
        repose_core::color::Transfer::Linear => 2.0,
        repose_core::color::Transfer::Pq => 3.0,
        repose_core::color::Transfer::Hlg => 4.0,
    };
    YuvTransformRaw {
        row0: [yuv.m[0][0], yuv.m[0][1], yuv.m[0][2], 0.0],
        row1: [yuv.m[1][0], yuv.m[1][1], yuv.m[1][2], 0.0],
        row2: [yuv.m[2][0], yuv.m[2][1], yuv.m[2][2], 0.0],
        b: [yuv.b[0], yuv.b[1], yuv.b[2], sample_scale],
        transfer: [transfer_mode, 0.0, 0.0, 0.0],
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Nv12Instance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    uv_x_offset: f32,
    uv_y_offset: f32,
    fwd_mat: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ClipInstance {
    xywh: [f32; 4],
    radii: [f32; 4],
    fwd_mat: [f32; 4],
}

/// Backdrop-blend composite instance: a `GlyphInstance`-shaped quad plus
/// the `BlendMode` discriminant consumed by `blend_layer.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlendInstance {
    xywh: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    fwd_mat: [f32; 4],
    mode: u32,
    _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshVertex {
    pos: [f32; 2],
    color: [f32; 4],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MeshUniform {
    m0: [f32; 4],
    m1: [f32; 4],
    paint: [u32; 4],
    color0: [f32; 4],
    color1: [f32; 4],
    grad_start: [f32; 2],
    _p3: [f32; 2],
    grad_end: [f32; 2],
    _p4: [f32; 2],
}

const MESH_UNIFORM_CAP: u64 = 4 * 1024 * 1024;
const MAX_RETAINED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RETAINED_IMAGES: usize = 512;
const MAX_BLEND_SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_GPU_RESOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_UPLOAD_RING_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CALLBACK_SCOPES: usize = 256;
const NV12_FOURCC: u32 = 0x3231_564e;
const P010_FOURCC: u32 = 0x3031_3050;

fn canonical_fourcc(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::P010 => P010_FOURCC,
        _ => NV12_FOURCC,
    }
}

impl MeshUniform {
    fn identity() -> Self {
        Self {
            m0: [1.0, 0.0, 0.0, 0.0],
            m1: [0.0, 1.0, 0.0, 0.0],
            paint: [0; 4],
            color0: [0.0; 4],
            color1: [0.0; 4],
            grad_start: [0.0; 2],
            _p3: [0.0; 2],
            grad_end: [0.0; 2],
            _p4: [0.0; 2],
        }
    }
}

fn mesh_uniform_from_paint(affine: [f32; 6], paint: &repose_core::PaintDesc) -> MeshUniform {
    let (paint_type, paint_kind, color0, color1, grad_start, grad_end) = match paint {
        repose_core::PaintDesc::Solid => (0u32, 0u32, [0.0; 4], [0.0; 4], [0.0; 2], [0.0; 2]),
        repose_core::PaintDesc::Linear {
            start,
            end,
            start_color,
            end_color,
        } => (
            1u32,
            0u32,
            start_color.to_linear(),
            end_color.to_linear(),
            [start.x, start.y],
            [end.x, end.y],
        ),
        repose_core::PaintDesc::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => (
            1u32,
            1u32,
            start_color.to_linear(),
            end_color.to_linear(),
            [center.x, center.y],
            [radius.max(0.0), 0.0],
        ),
        repose_core::PaintDesc::Sweep {
            center,
            start_color,
            end_color,
        } => (
            1u32,
            2u32,
            start_color.to_linear(),
            end_color.to_linear(),
            [center.x, center.y],
            [0.0, 0.0],
        ),
        _ => (0u32, 0u32, [0.0; 4], [0.0; 4], [0.0; 2], [0.0; 2]),
    };
    MeshUniform {
        m0: [affine[0], affine[1], affine[2], 0.0],
        m1: [affine[3], affine[4], affine[5], 0.0],
        paint: [paint_type, paint_kind, 0, 0],
        color0,
        color1,
        grad_start,
        _p3: [0.0; 2],
        grad_end,
        _p4: [0.0; 2],
    }
}

fn combine_mesh_affine(current: &Transform, mesh: [f32; 6]) -> [f32; 6] {
    let cm = current.linear();
    let (cm00, cm01, cm10, cm11) = (cm[0], cm[1], cm[2], cm[3]);
    let mm00 = mesh[0];
    let mm01 = mesh[1];
    let mm10 = mesh[2];
    let mm11 = mesh[3];
    let mtx = mesh[4];
    let mty = mesh[5];
    let r00 = cm00 * mm00 + cm01 * mm10;
    let r01 = cm00 * mm01 + cm01 * mm11;
    let r10 = cm10 * mm00 + cm11 * mm10;
    let r11 = cm10 * mm01 + cm11 * mm11;
    let tx = cm00 * mtx + cm01 * mty + current.translate_x;
    let ty = cm10 * mtx + cm11 * mty + current.translate_y;
    // Canonical slot order consumed by `MeshUniform`/shader and `mesh_aabb`:
    // [A, B, tx, C, D, ty] where world = [[A,B],[C,D]] * local + (tx, ty).
    [r00, r01, tx, r10, r11, ty]
}

fn mesh_aabb(mesh: &repose_core::VectorMeshData, affine: [f32; 6]) -> repose_core::Rect {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for v in mesh.vertices.iter() {
        let x = affine[0] * v.pos[0] + affine[1] * v.pos[1] + affine[2];
        let y = affine[3] * v.pos[0] + affine[4] * v.pos[1] + affine[5];
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    let w = (max_x - min_x).max(0.0);
    let h = (max_y - min_y).max(0.0);
    if !min_x.is_finite() || !min_y.is_finite() {
        return repose_core::Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    }
    repose_core::Rect {
        x: min_x,
        y: min_y,
        w,
        h,
    }
}

fn swash_to_a8_coverage(content: repose_text::SwashContent, data: &[u8]) -> Option<Vec<u8>> {
    match content {
        repose_text::SwashContent::Mask => Some(data.to_vec()),
        repose_text::SwashContent::SubpixelMask => {
            let mut out = Vec::with_capacity(data.len() / 4);
            for px in data.as_chunks::<4>().0 {
                let r = px[0];
                let g = px[1];
                let b = px[2];
                out.push(r.max(g).max(b));
            }
            Some(out)
        }
        repose_text::SwashContent::Color => None,
    }
}

impl WgpuSceneRenderer {
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        output_format: wgpu::TextureFormat,
        msaa_samples: u32,
    ) -> Self {
        Self::from_device_with_working_space_msaa(
            device,
            queue,
            output_format,
            msaa_samples,
            msaa_samples,
        )
    }

    pub fn from_device_with_working_space_msaa(
        device: wgpu::Device,
        queue: wgpu::Queue,
        output_format: wgpu::TextureFormat,
        msaa_samples: u32,
        working_space_msaa_samples: u32,
    ) -> Self {
        let msaa_samples = msaa_samples.max(1);
        let working_space_msaa_samples = working_space_msaa_samples.max(1);
        let mesh_uniform_size = std::mem::size_of::<MeshUniform>() as u64;
        let base_mesh_alignment =
            u64::from(device.limits().min_uniform_buffer_offset_alignment).max(4);
        let mesh_uniform_alignment =
            align_up(mesh_uniform_size, base_mesh_alignment).unwrap_or(base_mesh_alignment);
        let mesh_uniform_cap = align_up(
            MESH_UNIFORM_CAP
                .min(device.limits().max_buffer_size)
                .max(mesh_uniform_alignment),
            mesh_uniform_alignment,
        )
        .unwrap_or(mesh_uniform_alignment);
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

        let ds_format = wgpu::TextureFormat::Depth24PlusStencil8;

        let stencil_for_content = wgpu::DepthStencilState {
            format: ds_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    // Equal (not LessEqual): inverse (`Difference`) vector
                    // masks work by keeping the depth while incrementing the
                    // masked pixels, so content must test exact equality.
                    // Outcomes match LessEqual everywhere except pre-existing
                    // stencil leaks, which now fail visibly instead of
                    // drawing through (unbalanced clips already warn).
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
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::IncrementClamp,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::IncrementClamp,
                },
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
            bias: wgpu::DepthBiasState::default(),
        };

        let stencil_for_clip_dec = wgpu::DepthStencilState {
            format: ds_format,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::DecrementClamp,
                },
                back: wgpu::StencilFaceState {
                    compare: wgpu::CompareFunction::Always,
                    fail_op: wgpu::StencilOperation::Keep,
                    depth_fail_op: wgpu::StencilOperation::Keep,
                    pass_op: wgpu::StencilOperation::DecrementClamp,
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

        // linear filtering only blurs them; nearest keeps the blit crisp.
        let layer_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("layer nearest sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // Linear taps for Gaussian blur/shadow passes; nearest is kept for
        // the sharp 1:1 layer composite.
        let layer_sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("layer linear sampler"),
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

        // Layout for NV12 Images (TextureY + TextureUV + Sampler + YuvTransform uniform)
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
                    // YUV transform uniform buffer
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
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
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    shader_location: 2,
                    offset: 32,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };
        // Bind layout for per-draw vector mesh uniforms (dynamic offset).
        let mesh_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(mesh_uniform_size),
                },
                count: None,
            }],
        });
        let mesh_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh uniform buffer"),
            size: mesh_uniform_cap,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mesh_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh uniform bind"),
            layout: &mesh_bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &mesh_uniform_buf,
                    offset: 0,
                    size: NonZeroU64::new(mesh_uniform_size),
                }),
            }],
        });

        // Two sets of pipelines: one for the MSAA surface pass, one for layer
        // render-to-texture passes (sample_count = 1).
        let surface_pipes = Pipelines::create(
            &device,
            output_format,
            msaa_samples,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &stencil_for_clip_dec,
            &clip_vertex_layout,
            &mesh_bind_layout,
        );
        let working_space_pipes = Pipelines::create(
            &device,
            wgpu::TextureFormat::Rgba16Float,
            working_space_msaa_samples,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &stencil_for_clip_dec,
            &clip_vertex_layout,
            &mesh_bind_layout,
        );
        let layer_pipes = Pipelines::create(
            &device,
            output_format,
            1,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &stencil_for_clip_dec,
            &clip_vertex_layout,
            &mesh_bind_layout,
        );
        let working_space_layer_pipes = Pipelines::create(
            &device,
            wgpu::TextureFormat::Rgba16Float,
            1,
            &globals_layout,
            &text_bind_layout,
            &image_bind_layout_nv12,
            &clip_pipeline_layout,
            &stencil_for_content,
            &stencil_for_clip_inc,
            &stencil_for_clip_dec,
            &clip_vertex_layout,
            &mesh_bind_layout,
        );

        // Vector glyph rendering always available with tessellation+MSAA approach.
        let slug_enabled = true;

        // Blur composite ring (for graphics-layer drop shadows)
        let blur_ring = UploadRing::new(
            &device,
            "blur ring",
            1024 * 1024,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );

        // Atlases
        let atlas_mask = init_atlas_mask(&device);
        let atlas_color = init_atlas_color(&device);

        // Upload rings
        let ring_rect = UploadRing::new(
            &device,
            "ring rect",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_border = UploadRing::new(
            &device,
            "ring border",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_ellipse = UploadRing::new(
            &device,
            "ring ellipse",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_ellipse_border = UploadRing::new(
            &device,
            "ring ellipse border",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_arc = UploadRing::new(
            &device,
            "ring arc",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_glyph_mask = UploadRing::new(
            &device,
            "ring glyph mask",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_glyph_color = UploadRing::new(
            &device,
            "ring glyph color",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_slug = UploadRing::new(
            &device,
            "ring slug",
            1 << 22,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_clip = UploadRing::new(
            &device,
            "ring clip",
            1 << 16,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let blend_ring = UploadRing::new(
            &device,
            "ring blend",
            1 << 16,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_projective = UploadRing::new(
            &device,
            "ring projective",
            1 << 16,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_nv12 = UploadRing::new(
            &device,
            "ring nv12",
            1 << 20,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_mesh_verts = UploadRing::new(
            &device,
            "ring mesh verts",
            1 << 22,
            wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let ring_mesh_indices = UploadRing::new(
            &device,
            "ring mesh indices",
            1 << 22,
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        );

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

        let mut renderer = WgpuSceneRenderer {
            device,
            queue,
            output_format,
            output_width: 0,
            output_height: 0,
            pixels_per_point: 1.0,

            surface_pipes,
            working_space_pipes,
            layer_pipes,
            working_space_layer_pipes,

            rects: InstancedPipe::new(ring_rect),
            borders: InstancedPipe::new(ring_border),
            ellipses: InstancedPipe::new(ring_ellipse),
            ellipse_borders: InstancedPipe::new(ring_ellipse_border),
            arcs: InstancedPipe::new(ring_arc),
            glyph_mask: InstancedPipe::new(ring_glyph_mask),
            glyph_color: InstancedPipe::new(ring_glyph_color),

            text_bind_layout,

            image_bind_layout_rgba,
            image_bind_layout_nv12,
            image_sampler,
            layer_sampler,
            layer_sampler_linear,

            blur_ring,

            slug_enabled,
            slug_ring: ring_slug,
            slug_cache: slug::GlyphSlugCache::new(),

            clip_ring: ring_clip,

            nv12: InstancedPipe::new(ring_nv12),

            mesh_verts: ring_mesh_verts,
            mesh_indices: ring_mesh_indices,
            mesh_uniform_buf,
            mesh_bind,
            mesh_uniform_head: 0,
            mesh_uniform_alignment,
            mesh_uniform_cap,

            projective_ring: ring_projective,
            blend_ring,
            flatten_layer_ids: Vec::new(),
            blend_snapshots: std::collections::HashMap::new(),
            blend_copies: Vec::new(),

            msaa_samples,
            working_space_msaa_samples,
            depth_stencil_tex,
            depth_stencil_view,
            msaa_tex: None,
            msaa_view: None,
            ws_msaa_tex: None,
            ws_msaa_view: None,
            surface_resolve_tex: None,
            surface_resolve_view: None,
            surface_resolve_bytes: 0,
            msaa_bytes: 0,
            ws_msaa_bytes: 0,
            depth_stencil_bytes: 0,
            working_space_bytes: 0,
            gpu_budget_bytes: MAX_GPU_RESOURCE_BYTES,
            globals_bind,
            globals_buf,

            atlas_mask,
            atlas_color,

            next_image_handle: 1,
            images: HashMap::new(),
            retained: HashMap::new(),
            retained_bytes_total: 0,

            next_coverage_handle: 1,
            coverages: HashMap::new(),

            frame_index: 0,
            image_bytes_total: 0,
            image_evict_after_frames: 600,         // ~10s @ 60fps
            image_budget_bytes: 512 * 1024 * 1024, // 512 MB
            layer_pool: HashMap::new(),
            producer_layer_ids: Vec::new(),
            layer_bytes_total: 0,

            working_space: false,
            ws_tex: None,
            ws_view: None,
            ws_bind: None,
            display_pipeline: None,
            display_layout: None,

            callback_resources: CallbackResources::default(),
            callback_scoped_resources: HashMap::new(),
            callback_scope_uses: HashMap::new(),
            callback_scope_payloads: HashMap::new(),
            callback_scope_clock: 0,
            blend_snapshot_bytes_total: 0,
            frame_active: false,
            last_render_error: None,
        };

        renderer.recreate_msaa_and_depth_stencil();
        renderer
    }
}

impl WgpuSurfaceBackend {
    #[cfg(feature = "winit-surface")]
    pub async fn new_async(
        window: Arc<winit::window::Window>,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        Self::new_async_with_options(window, 4, PresentModePref::Auto).await
    }

    /// Create a windowed surface backend, honoring the requested MSAA sample
    /// count (falling back to the largest supported count <= `msaa_samples`).
    #[cfg(feature = "winit-surface")]
    pub async fn new_async_with_msaa(
        window: Arc<winit::window::Window>,
        msaa_samples: u32,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        Self::new_async_with_options(window, msaa_samples, PresentModePref::Auto).await
    }

    /// Create a windowed surface backend, honoring the requested MSAA sample
    /// count and present-mode preference.
    #[cfg(feature = "winit-surface")]
    pub async fn new_async_with_options(
        window: Arc<winit::window::Window>,
        msaa_samples: u32,
        present_mode: PresentModePref,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        let instance: Instance = if cfg!(target_arch = "wasm32") {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            wgpu::util::new_instance_with_webgpu_detection(desc).await
        } else {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
        };

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| anyhow::anyhow!("No suitable adapter: {e:?}"))?;

        let limits = adapter.limits();

        let features = {
            let available = adapter.features();
            let mut features = wgpu::Features::empty();
            if available.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM) {
                features |= wgpu::Features::TEXTURE_FORMAT_16BIT_NORM;
            }
            #[cfg(target_os = "linux")]
            {
                if available.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_FD) {
                    features |= wgpu::Features::VULKAN_EXTERNAL_MEMORY_FD;
                }
                if available.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF) {
                    features |= wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF;
                }
            }
            features
        };

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("repose-rs device"),
                required_features: features,
                required_limits: limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| anyhow::anyhow!("request_device failed: {e:?}"))?;

        let size = window.inner_size();

        let caps = surface.get_capabilities(&adapter);

        let (format, view_format) = if cfg!(target_arch = "wasm32")
            && adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS)
        {
            let non_srgb = caps
                .formats
                .iter()
                .copied()
                .find(|f| !f.is_srgb())
                .unwrap_or(caps.formats[0]);
            (non_srgb, Some(non_srgb.add_srgb_suffix()))
        } else if cfg!(target_arch = "wasm32") {
            let fmt = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            (fmt, None)
        } else {
            let fmt = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            (fmt, None)
        };

        let present_mode = pick_present_mode(&caps, present_mode);
        let alpha_mode = caps.alpha_modes[0];

        let render_format = view_format.unwrap_or(format);
        let msaa_samples = pick_surface_msaa(&adapter, render_format, msaa_samples);
        let working_space_msaa_samples = pick_surface_msaa_for_mode(
            &adapter,
            wgpu::TextureFormat::Rgba16Float,
            msaa_samples,
            true,
        );
        let mut renderer = WgpuSceneRenderer::from_device_with_working_space_msaa(
            device,
            queue,
            render_format,
            msaa_samples,
            working_space_msaa_samples,
        );
        renderer.resize(size.width, size.height);

        let view_formats = view_format.into_iter().collect::<Vec<_>>();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats,
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&renderer.device, &config);

        Ok(WgpuSurfaceBackend {
            #[cfg(feature = "winit-surface")]
            instance: Some(instance),
            #[cfg(feature = "winit-surface")]
            window: Some(window),
            surface: Some(surface),
            surface_config: Some(config),
            pending_reconfigure: false,
            renderer,
        })
    }

    #[cfg(all(feature = "winit-surface", not(target_arch = "wasm32")))]
    pub fn new(window: Arc<winit::window::Window>) -> anyhow::Result<WgpuSurfaceBackend> {
        pollster::block_on(Self::new_async(window))
    }

    #[cfg(all(feature = "winit-surface", not(target_arch = "wasm32")))]
    pub fn new_with_msaa(
        window: Arc<winit::window::Window>,
        msaa_samples: u32,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        pollster::block_on(Self::new_async_with_msaa(window, msaa_samples))
    }

    #[cfg(all(feature = "winit-surface", not(target_arch = "wasm32")))]
    pub fn new_with_options(
        window: Arc<winit::window::Window>,
        msaa_samples: u32,
        present_mode: PresentModePref,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        pollster::block_on(Self::new_async_with_options(
            window,
            msaa_samples,
            present_mode,
        ))
    }

    #[cfg(all(feature = "winit-surface", target_arch = "wasm32"))]
    pub fn new(_window: Arc<winit::window::Window>) -> anyhow::Result<WgpuSurfaceBackend> {
        anyhow::bail!("Use WgpuSurfaceBackend::new_async(window).await on wasm32")
    }

    #[cfg(all(feature = "winit-surface", target_arch = "wasm32"))]
    pub fn new_with_msaa(
        _window: Arc<winit::window::Window>,
        _msaa_samples: u32,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        anyhow::bail!("Use WgpuSurfaceBackend::new_async_with_msaa(window, msaa).await on wasm32")
    }

    #[cfg(all(feature = "winit-surface", target_arch = "wasm32"))]
    pub fn new_with_options(
        _window: Arc<winit::window::Window>,
        _msaa_samples: u32,
        _present_mode: PresentModePref,
    ) -> anyhow::Result<WgpuSurfaceBackend> {
        anyhow::bail!(
            "Use WgpuSurfaceBackend::new_async_with_options(window, msaa, mode).await on wasm32"
        )
    }
}

/// Pick the swapchain present mode honoring `pref`, falling back to an "auto"
/// Fifo-first selection when the preferred mode is unavailable.
#[cfg(feature = "winit-surface")]
fn pick_present_mode(caps: &wgpu::SurfaceCapabilities, pref: PresentModePref) -> wgpu::PresentMode {
    let auto = || {
        caps.present_modes
            .iter()
            .copied()
            .find(|m| *m == wgpu::PresentMode::Fifo)
            .or_else(|| {
                caps.present_modes
                    .iter()
                    .copied()
                    .find(|m| *m == wgpu::PresentMode::Mailbox)
            })
            .unwrap_or(wgpu::PresentMode::Immediate)
    };
    match pref {
        PresentModePref::Auto => auto(),
        PresentModePref::Fifo if caps.present_modes.contains(&wgpu::PresentMode::Fifo) => {
            wgpu::PresentMode::Fifo
        }
        PresentModePref::Mailbox if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) => {
            wgpu::PresentMode::Mailbox
        }
        PresentModePref::Immediate
            if caps.present_modes.contains(&wgpu::PresentMode::Immediate) =>
        {
            wgpu::PresentMode::Immediate
        }
        _ => auto(),
    }
}

/// Pick the MSAA sample count for the surface pass, honoring `requested` and
/// falling back to the largest supported count <= it.
pub fn pick_surface_msaa(
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    requested: u32,
) -> u32 {
    pick_surface_msaa_for_mode(adapter, format, requested, false)
}

pub fn pick_surface_msaa_for_mode(
    adapter: &wgpu::Adapter,
    format: wgpu::TextureFormat,
    requested: u32,
    working_space: bool,
) -> u32 {
    let requested = requested.max(1);
    let color_feat = adapter.get_texture_format_features(format);
    let working_space_feat = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
    let depth_feat = adapter.get_texture_format_features(wgpu::TextureFormat::Depth24PlusStencil8);
    let supported = |n: u32| {
        color_feat
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            && color_feat.flags.sample_count_supported(n)
            && (!working_space
                || (working_space_feat
                    .allowed_usages
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
                    && working_space_feat.flags.sample_count_supported(n)
                    && (n == 1
                        || working_space_feat
                            .flags
                            .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))))
            && (n == 1
                || color_feat
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
            && depth_feat
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            && depth_feat.flags.sample_count_supported(n)
    };
    let mut candidates = vec![requested];
    for n in [8, 4, 2, 1] {
        if n < requested {
            candidates.push(n);
        }
    }
    let chosen = candidates.into_iter().find(|&n| supported(n)).unwrap_or(1);
    if chosen != requested {
        log::info!("requested MSAA x{requested}, using x{chosen}");
    }
    chosen
}

impl WgpuSceneRenderer {
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
        validate_image_handle(handle)?;
        validate_texture_dimensions(&self.device, w, h)?;
        let expected = checked_image_bytes_usize(w, h, 4)?;
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
            self.remove_image(handle);

            let (tex, bind) = self.create_rgba_tex(w, h, format);
            let bytes = checked_image_bytes(w, h, 4)?;
            self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);

            self.images.insert(
                handle,
                ImageTex::Rgba {
                    tex,
                    bind,
                    w,
                    h,
                    format,
                    last_used_frame: self.frame_index,
                    bytes,
                },
            );
        }

        if let Some(old) = self.retained.remove(&handle) {
            self.retained_bytes_total = self
                .retained_bytes_total
                .saturating_sub(old.rgba.len() as u64);
        }
        self.retained_bytes_total = self.retained_bytes_total.saturating_add(expected as u64);
        self.retained.insert(
            handle,
            RetainedImage {
                w,
                h,
                format,
                rgba: rgba[..expected].to_vec(),
                last_used_frame: self.frame_index,
            },
        );
        self.evict_retained_excess();

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

    /// Create (but do not populate) the GPU texture, view and bind group for an
    /// RGBA image. Pixels are written separately via `write_texture`.
    fn create_rgba_tex(
        &self,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
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

        (tex, bind)
    }

    /// Register an externally-created `wgpu::TextureView` as an image (zero-copy).
    fn validate_native_texture_view(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            anyhow::bail!("native texture dimensions must be non-zero");
        }
        let texture = view.texture();
        if texture.sample_count() != 1
            || texture.dimension() != wgpu::TextureDimension::D2
            || texture.depth_or_array_layers() != 1
            || texture.mip_level_count() == 0
        {
            anyhow::bail!("native texture must be a single-sample 2D texture with mip levels");
        }
        if width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            anyhow::bail!("native texture dimensions exceed the device limit");
        }
        if !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            anyhow::bail!("native texture lacks TEXTURE_BINDING usage");
        }
        if !native_format_supported(texture.format(), self.device.features()) {
            anyhow::bail!(
                "native texture format {:?} is not a filterable alpha color format",
                texture.format()
            );
        }
        if texture.width() < width || texture.height() < height {
            anyhow::bail!(
                "native texture is {}x{}, smaller than requested {width}x{height}",
                texture.width(),
                texture.height()
            );
        }
        Ok(())
    }

    pub fn register_native_texture(
        &mut self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> u64 {
        if let Err(error) = self.validate_native_texture_view(view, width, height) {
            log::warn!("register_native_texture: {error:#}");
            return 0;
        }
        let handle = self.next_image_handle;
        self.next_image_handle += 1;
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("user native image"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
            ],
        });
        self.images.insert(
            handle,
            ImageTex::User {
                bind,
                w: width,
                h: height,
                last_used_frame: self.frame_index,
                bytes: 0,
            },
        );
        handle
    }

    /// Like `register_native_texture` but with custom sampler descriptor.
    pub fn register_native_texture_with_sampler(
        &mut self,
        view: &wgpu::TextureView,
        sampler_desc: wgpu::SamplerDescriptor<'_>,
        width: u32,
        height: u32,
    ) -> u64 {
        if let Err(error) = self.validate_native_texture_view(view, width, height) {
            log::warn!("register_native_texture: {error:#}");
            return 0;
        }
        if let Err(error) = validate_sampler_descriptor(&self.device, &sampler_desc) {
            log::warn!("register_native_texture: {error:#}");
            return 0;
        }
        let handle = self.next_image_handle;
        self.next_image_handle += 1;
        let sampler = self.device.create_sampler(&sampler_desc);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("user native image sampleropts"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        self.images.insert(
            handle,
            ImageTex::User {
                bind,
                w: width,
                h: height,
                last_used_frame: self.frame_index,
                bytes: 0,
            },
        );
        handle
    }

    /// Update an existing native texture handle with a new view (reuse handle).
    pub fn update_native_texture(&mut self, handle: u64, view: &wgpu::TextureView) {
        if handle == 0 {
            log::warn!("update_native_texture: reserved handle");
            return;
        }
        let Some((w, h)) = self.images.get(&handle).and_then(|entry| match entry {
            ImageTex::User { w, h, .. } => Some((*w, *h)),
            _ => None,
        }) else {
            log::warn!("update_native_texture: handle {handle} is not a native image");
            return;
        };
        if let Err(error) = self.validate_native_texture_view(view, w, h) {
            log::warn!("update_native_texture: {error:#}");
            return;
        }
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("user native image update"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
            ],
        });
        if let Some(entry) = self.images.get_mut(&handle) {
            *entry = ImageTex::User {
                bind,
                w,
                h,
                last_used_frame: self.frame_index,
                bytes: 0,
            };
        }
    }

    pub fn set_image_nv12(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        y: &[u8],
        uv: &[u8],
        color_info: ColorInfo,
    ) -> anyhow::Result<()> {
        validate_image_handle(handle)?;
        validate_texture_dimensions(&self.device, w, h)?;
        if self
            .images
            .get(&handle)
            .is_some_and(|image| matches!(image, ImageTex::Nv12 { external: true, .. }))
        {
            anyhow::bail!("cannot overwrite an external DMA-BUF image");
        }
        let y_expected = checked_image_bytes_usize(w, h, 1)?;
        let uv_w = w.div_ceil(2);
        let uv_h = h.div_ceil(2);
        let uv_expected = checked_image_bytes_usize(uv_w, uv_h, 2)?;

        if y.len() < y_expected {
            return Err(anyhow::anyhow!("Y plane too small"));
        }
        if uv.len() < uv_expected {
            return Err(anyhow::anyhow!("UV plane too small"));
        }

        let needs_recreate = match self.images.get(&handle) {
            Some(ImageTex::Nv12 {
                w: ww,
                h: hh,
                pixel_format,
                ..
            }) => *ww != w || *hh != h || *pixel_format != PixelFormat::Nv12,
            _ => true,
        };

        // Compute the YUV->RGB transform on the CPU.
        let yuv_raw = make_yuv_transform_raw(color_info, PixelFormat::Nv12);

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

            // Create a uniform buffer for the YUV transform (per-image).
            let yuv_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nv12 yuv transform"),
                size: std::mem::size_of::<YuvTransformRaw>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Write initial transform.
            self.queue
                .write_buffer(&yuv_buf, 0, bytemuck::bytes_of(&yuv_raw));

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
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &yuv_buf,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });

            let bytes = checked_image_bytes(w, h, 1)?
                .checked_add(checked_image_bytes(uv_w, uv_h, 2)?)
                .and_then(|bytes| bytes.checked_add(std::mem::size_of::<YuvTransformRaw>() as u64))
                .ok_or_else(|| anyhow::anyhow!("NV12 image byte size overflow"))?;
            self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);

            self.images.insert(
                handle,
                ImageTex::Nv12 {
                    tex_y,
                    tex_uv,
                    bind,
                    yuv_buf,
                    w,
                    h,
                    pixel_format: PixelFormat::Nv12,
                    fourcc: Some(NV12_FOURCC),
                    color_info,
                    external: false,
                    last_used_frame: self.frame_index,
                    bytes,
                },
            );
        } else {
            // Re-use existing textures; just update the YUV transform if needed.
            if let Some(ImageTex::Nv12 {
                yuv_buf,
                color_info: stored_color,
                last_used_frame,
                ..
            }) = self.images.get_mut(&handle)
            {
                *stored_color = color_info;
                *last_used_frame = self.frame_index;
                self.queue
                    .write_buffer(yuv_buf, 0, bytemuck::bytes_of(&yuv_raw));
            }
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

    pub fn set_image_planes(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        pixel_format: PixelFormat,
        planes: &[&[u8]],
        color_info: ColorInfo,
    ) -> anyhow::Result<()> {
        validate_image_handle(handle)?;
        if planes.len() != pixel_format.num_planes() as usize {
            anyhow::bail!(
                "{} requires {} planes, got {}",
                match pixel_format {
                    PixelFormat::Nv12 => "NV12",
                    PixelFormat::P010 => "P010",
                    PixelFormat::I420 => "I420",
                    PixelFormat::I444 => "I444",
                    PixelFormat::Rgba => "RGBA",
                },
                pixel_format.num_planes(),
                planes.len()
            );
        }
        match pixel_format {
            PixelFormat::Nv12 => {
                let y = planes.first().ok_or(anyhow::anyhow!("missing Y plane"))?;
                let uv = planes.get(1).ok_or(anyhow::anyhow!("missing UV plane"))?;
                self.set_image_nv12(handle, w, h, y, uv, color_info)
            }
            PixelFormat::P010 => {
                let y = planes.first().ok_or(anyhow::anyhow!("missing Y plane"))?;
                let uv = planes.get(1).ok_or(anyhow::anyhow!("missing UV plane"))?;
                self.set_image_p010(handle, w, h, y, uv, color_info)
            }
            PixelFormat::I420 | PixelFormat::I444 => Err(anyhow::anyhow!(
                "I420/I444 not implemented and unlikely -> cheap to convert to NV12 (better for the GPU too)"
            )),
            PixelFormat::Rgba => {
                let rgba = planes
                    .first()
                    .ok_or(anyhow::anyhow!("missing RGBA plane"))?;
                self.set_image_rgba8(handle, w, h, rgba, false)
            }
        }
    }

    fn set_image_p010(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        y: &[u8],
        uv: &[u8],
        color_info: ColorInfo,
    ) -> anyhow::Result<()> {
        validate_image_handle(handle)?;
        validate_texture_dimensions(&self.device, w, h)?;
        if self
            .images
            .get(&handle)
            .is_some_and(|image| matches!(image, ImageTex::Nv12 { external: true, .. }))
        {
            anyhow::bail!("cannot overwrite an external DMA-BUF image");
        }
        if !self
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
        {
            anyhow::bail!("P010 requires TEXTURE_FORMAT_16BIT_NORM");
        }
        let uv_w = w.div_ceil(2);
        let uv_h = h.div_ceil(2);

        let y_expected = checked_image_bytes_usize(w, h, 2)?;
        let uv_expected = checked_image_bytes_usize(uv_w, uv_h, 4)?;

        if y.len() < y_expected {
            return Err(anyhow::anyhow!("P010 Y plane too small"));
        }
        if uv.len() < uv_expected {
            return Err(anyhow::anyhow!("P010 UV plane too small"));
        }

        // P010 reuses the NV12 pipeline (same bind group layout -> wgpu
        // abstracts the storage format so R16Unorm/Rg16Unorm are
        // filterable float textures just like R8Unorm/Rg8Unorm).
        let needs_recreate = match self.images.get(&handle) {
            Some(ImageTex::Nv12 {
                w: ww,
                h: hh,
                pixel_format,
                ..
            }) => *ww != w || *hh != h || *pixel_format != PixelFormat::P010,
            _ => true,
        };

        let yuv_raw = make_yuv_transform_raw(color_info, PixelFormat::P010);

        if needs_recreate {
            self.remove_image(handle);

            let tex_y = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("p010 Y"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view_y = tex_y.create_view(&wgpu::TextureViewDescriptor::default());

            let tex_uv = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("p010 UV"),
                size: wgpu::Extent3d {
                    width: uv_w,
                    height: uv_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg16Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view_uv = tex_uv.create_view(&wgpu::TextureViewDescriptor::default());

            let yuv_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("p010 yuv transform"),
                size: std::mem::size_of::<YuvTransformRaw>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&yuv_buf, 0, bytemuck::bytes_of(&yuv_raw));

            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("p010 bind"),
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
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &yuv_buf,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });

            let bytes = checked_image_bytes(w, h, 2)?
                .checked_add(checked_image_bytes(uv_w, uv_h, 4)?)
                .and_then(|bytes| bytes.checked_add(std::mem::size_of::<YuvTransformRaw>() as u64))
                .ok_or_else(|| anyhow::anyhow!("P010 image byte size overflow"))?;
            self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);

            self.images.insert(
                handle,
                ImageTex::Nv12 {
                    tex_y,
                    tex_uv,
                    bind,
                    yuv_buf,
                    w,
                    h,
                    pixel_format: PixelFormat::P010,
                    fourcc: Some(P010_FOURCC),
                    color_info,
                    external: false,
                    last_used_frame: self.frame_index,
                    bytes,
                },
            );
        } else {
            if let Some(ImageTex::Nv12 {
                yuv_buf,
                color_info: stored_color,
                last_used_frame,
                ..
            }) = self.images.get_mut(&handle)
            {
                *stored_color = color_info;
                *last_used_frame = self.frame_index;
                self.queue
                    .write_buffer(yuv_buf, 0, bytemuck::bytes_of(&yuv_raw));
            }
        }

        let (tex_y, tex_uv, _bind) = match self.images.get(&handle) {
            Some(ImageTex::Nv12 {
                tex_y,
                tex_uv,
                bind,
                ..
            }) => (tex_y, tex_uv, bind),
            _ => return Err(anyhow::anyhow!("Handle is not P010/NV12")),
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
                bytes_per_row: Some(
                    w.checked_mul(2)
                        .ok_or_else(|| anyhow::anyhow!("P010 row size overflow"))?,
                ),
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
                bytes_per_row: Some(
                    uv_w.checked_mul(4)
                        .ok_or_else(|| anyhow::anyhow!("P010 UV row size overflow"))?,
                ),
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

    #[cfg(target_os = "linux")]
    pub fn set_image_dmabuf(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        fds: Vec<std::os::unix::io::OwnedFd>,
        modifier: u64,
        strides: Vec<u32>,
        offsets: Vec<u64>,
        color_info: ColorInfo,
    ) -> anyhow::Result<()> {
        self.set_image_dmabuf_fourcc(
            handle,
            w,
            h,
            fds,
            NV12_FOURCC,
            modifier,
            strides,
            offsets,
            color_info,
        )
    }

    #[cfg(target_os = "linux")]
    pub fn set_image_dmabuf_fourcc(
        &mut self,
        handle: u64,
        w: u32,
        h: u32,
        fds: Vec<std::os::unix::io::OwnedFd>,
        fourcc: u32,
        modifier: u64,
        strides: Vec<u32>,
        offsets: Vec<u64>,
        color_info: ColorInfo,
    ) -> anyhow::Result<()> {
        validate_image_handle(handle)?;
        checked_image_bytes(w, h, 1)?;
        if w == 0
            || h == 0
            || w > self.device.limits().max_texture_dimension_2d
            || h > self.device.limits().max_texture_dimension_2d
        {
            anyhow::bail!("DMA-BUF dimensions are outside the device texture limit");
        }
        if modifier == u64::MAX {
            anyhow::bail!("DMA-BUF modifier is invalid");
        }
        if fds.len() != 2 || strides.len() != 2 || offsets.len() != 2 {
            anyhow::bail!(
                "DMA-BUF import requires two fds, strides, and offsets (got {}, {}, {})",
                fds.len(),
                strides.len(),
                offsets.len()
            );
        }
        let pixel_format = match fourcc {
            0x3231_564e | 0x4e56_3132 => PixelFormat::Nv12,
            0x3031_3050 | 0x5030_3130 => PixelFormat::P010,
            _ => anyhow::bail!("unsupported DMA-BUF fourcc 0x{fourcc:08x}"),
        };
        if !self
            .device
            .features()
            .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
        {
            anyhow::bail!("DMA-BUF import requires VULKAN_EXTERNAL_MEMORY_DMA_BUF");
        }
        if pixel_format == PixelFormat::P010
            && !self
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
        {
            anyhow::bail!("P010 DMA-BUF requires TEXTURE_FORMAT_16BIT_NORM");
        }
        let bytes_per_pixel = if pixel_format == PixelFormat::P010 {
            2
        } else {
            1
        };
        let uv_w = w.div_ceil(2);
        let uv_h = h.div_ceil(2);
        let y_file_size =
            validate_dmabuf_plane(&fds[0], offsets[0], strides[0], w, h, bytes_per_pixel)?;
        let uv_file_size = validate_dmabuf_plane(
            &fds[1],
            offsets[1],
            strides[1],
            uv_w,
            uv_h,
            bytes_per_pixel * 2,
        )?;
        let yuv_raw = make_yuv_transform_raw(color_info, pixel_format);
        let (y_format, uv_format) = if pixel_format == PixelFormat::P010 {
            (
                wgpu::TextureFormat::R16Unorm,
                wgpu::TextureFormat::Rg16Unorm,
            )
        } else {
            (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm)
        };
        let hal_y_desc = wgpu::hal::TextureDescriptor {
            label: Some("dmabuf y"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: y_format,
            usage: wgpu::wgt::TextureUses::RESOURCE,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let hal_uv_desc = wgpu::hal::TextureDescriptor {
            label: Some("dmabuf uv"),
            size: wgpu::Extent3d {
                width: uv_w,
                height: uv_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: uv_format,
            usage: wgpu::wgt::TextureUses::RESOURCE,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let wgpu_y_desc = wgpu::TextureDescriptor {
            label: Some("dmabuf y"),
            size: hal_y_desc.size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: y_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let wgpu_uv_desc = wgpu::TextureDescriptor {
            label: Some("dmabuf uv"),
            size: hal_uv_desc.size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: uv_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let (tex_y, view_y, tex_uv, view_uv) = unsafe {
            let hal_guard = self
                .device
                .as_hal::<wgpu::hal::vulkan::Api>()
                .ok_or_else(|| anyhow::anyhow!("device is not Vulkan"))?;
            let mut fds = fds.into_iter();
            let y_fd = fds.next().expect("validated fd count");
            let uv_fd = fds.next().expect("validated fd count");
            let y_texture = hal_guard.texture_from_dmabuf_fd(
                y_fd,
                &hal_y_desc,
                modifier,
                strides[0] as u64,
                offsets[0],
            );
            let y_texture =
                y_texture.map_err(|error| anyhow::anyhow!("import Y DMA-BUF: {error:?}"))?;
            let uv_texture = hal_guard.texture_from_dmabuf_fd(
                uv_fd,
                &hal_uv_desc,
                modifier,
                strides[1] as u64,
                offsets[1],
            );
            let uv_texture =
                uv_texture.map_err(|error| anyhow::anyhow!("import UV DMA-BUF: {error:?}"))?;
            drop(hal_guard);
            let tex_y = self
                .device
                .create_texture_from_hal::<wgpu::hal::vulkan::Api>(
                    y_texture,
                    &wgpu_y_desc,
                    wgpu::wgt::TextureUses::UNINITIALIZED,
                );
            let tex_uv = self
                .device
                .create_texture_from_hal::<wgpu::hal::vulkan::Api>(
                    uv_texture,
                    &wgpu_uv_desc,
                    wgpu::wgt::TextureUses::UNINITIALIZED,
                );
            let view_y = tex_y.create_view(&wgpu::TextureViewDescriptor::default());
            let view_uv = tex_uv.create_view(&wgpu::TextureViewDescriptor::default());
            (tex_y, view_y, tex_uv, view_uv)
        };

        let yuv_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dmabuf yuv transform"),
            size: std::mem::size_of::<YuvTransformRaw>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue
            .write_buffer(&yuv_buf, 0, bytemuck::bytes_of(&yuv_raw));
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dmabuf yuv bind"),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &yuv_buf,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });
        let bytes = y_file_size
            .checked_add(uv_file_size)
            .and_then(|value| value.checked_add(std::mem::size_of::<YuvTransformRaw>() as u64))
            .ok_or_else(|| anyhow::anyhow!("DMA-BUF byte size overflow"))?;
        self.evict_budget_excess();
        let replaced_bytes = self
            .images
            .get(&handle)
            .map(|image| match image {
                ImageTex::Rgba { bytes, .. }
                | ImageTex::Nv12 { bytes, .. }
                | ImageTex::User { bytes, .. } => *bytes,
            })
            .unwrap_or(0);
        let projected = self
            .image_bytes_total
            .saturating_sub(replaced_bytes)
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("DMA-BUF image budget overflow"))?;
        if projected > self.image_budget_bytes {
            anyhow::bail!("DMA-BUF image budget exceeded");
        }
        self.remove_image(handle);
        self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);
        self.images.insert(
            handle,
            ImageTex::Nv12 {
                tex_y,
                tex_uv,
                bind,
                yuv_buf,
                w,
                h,
                pixel_format,
                fourcc: Some(canonical_fourcc(pixel_format)),
                color_info,
                external: true,
                last_used_frame: self.frame_index,
                bytes,
            },
        );
        self.evict_budget_excess();
        Ok(())
    }

    pub fn image_fourcc(&self, handle: u64) -> Option<u32> {
        match self.images.get(&handle) {
            Some(ImageTex::Nv12 { fourcc, .. }) => *fourcc,
            _ => None,
        }
    }

    pub fn remove_image(&mut self, handle: u64) {
        if let Some(img) = self.images.remove(&handle) {
            let b = match &img {
                ImageTex::Rgba { bytes, .. } => *bytes,
                ImageTex::Nv12 { bytes, .. } => *bytes,
                ImageTex::User { bytes, .. } => *bytes,
            };
            self.image_bytes_total = self.image_bytes_total.saturating_sub(b);
        }
        if let Some(retained) = self.retained.remove(&handle) {
            self.retained_bytes_total = self
                .retained_bytes_total
                .saturating_sub(retained.rgba.len() as u64);
        }
    }

    fn evict_image_gpu(&mut self, handle: u64) -> u64 {
        let Some(img) = self.images.remove(&handle) else {
            return 0;
        };
        let b = match &img {
            ImageTex::Rgba { bytes, .. } => *bytes,
            ImageTex::Nv12 { bytes, .. } => *bytes,
            ImageTex::User { bytes, .. } => *bytes,
        };
        self.image_bytes_total = self.image_bytes_total.saturating_sub(b);
        b
    }

    fn revive_retained_image(&mut self, handle: u64) -> anyhow::Result<bool> {
        if self.images.contains_key(&handle) {
            return Ok(true);
        }
        let Some(r) = self.retained.get_mut(&handle) else {
            return Ok(false);
        };
        r.last_used_frame = self.frame_index;
        let r = r.clone();
        let (tex, bind) = self.create_rgba_tex(r.w, r.h, r.format);

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &r.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * r.w),
                rows_per_image: Some(r.h),
            },
            wgpu::Extent3d {
                width: r.w,
                height: r.h,
                depth_or_array_layers: 1,
            },
        );

        let bytes = checked_image_bytes(r.w, r.h, 4)?;
        self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);
        self.images.insert(
            handle,
            ImageTex::Rgba {
                tex,
                bind,
                w: r.w,
                h: r.h,
                format: r.format,
                last_used_frame: self.frame_index,
                bytes,
            },
        );
        Ok(true)
    }

    fn resolve_image_for_draw(&mut self, handle: u64) -> Option<(u32, u32, bool)> {
        if let Some(retained) = self.retained.get_mut(&handle) {
            retained.last_used_frame = self.frame_index;
        }
        if let Some(t) = self.images.get_mut(&handle) {
            return match t {
                ImageTex::Rgba {
                    w,
                    h,
                    last_used_frame,
                    ..
                } => {
                    *last_used_frame = self.frame_index;
                    Some((*w, *h, false))
                }
                ImageTex::User {
                    w,
                    h,
                    last_used_frame,
                    ..
                } => {
                    *last_used_frame = self.frame_index;
                    Some((*w, *h, false))
                }
                ImageTex::Nv12 {
                    w,
                    h,
                    last_used_frame,
                    ..
                } => {
                    *last_used_frame = self.frame_index;
                    Some((*w, *h, true))
                }
            };
        }
        if self.revive_retained_image(handle).unwrap_or(false)
            && let Some(ImageTex::Rgba {
                w,
                h,
                last_used_frame,
                ..
            }) = self.images.get_mut(&handle)
        {
            *last_used_frame = self.frame_index;
            return Some((*w, *h, false));
        }
        None
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

    /// Register raw RGBA8 pixels (`w * h * 4` bytes) as an image, returning
    /// its handle for `SceneNode::Image`. Used by CPU-rasterized overlays
    /// (e.g. subtitle bitmap layers) that have no encoded image bytes.
    /// Pass `srgb = true` for sRGB overlays composited over video.
    pub fn register_image_rgba8(&mut self, w: u32, h: u32, rgba: &[u8], srgb: bool) -> u64 {
        let handle = self.next_image_handle;
        self.next_image_handle += 1;
        if let Err(e) = self.set_image_rgba8(handle, w, h, rgba, srgb) {
            log::error!("Failed to register image: {e}");
        }
        handle
    }

    /// Register an 8-bit coverage tile (`w * h` bytes, 0 = empty, 255 =
    /// fully covered) for `SceneNode::Coverage`, returning its handle.
    /// Coverage tiles are immutable: re-register on geometry change and
    /// `remove_coverage` handles you no longer emit (stale tiles also age
    /// out under the image eviction policy).
    pub fn register_coverage_a8(&mut self, w: u32, h: u32, coverage: &[u8]) -> u64 {
        if validate_texture_dimensions(&self.device, w, h).is_err() {
            log::error!("Coverage dimensions are outside the device limit");
            return 0;
        }
        let Ok(expected_bytes) = checked_image_bytes(w, h, 1) else {
            log::error!("Coverage dimensions overflow");
            return 0;
        };
        let Ok(expected) = usize::try_from(expected_bytes) else {
            log::error!("Coverage dimensions exceed addressable memory");
            return 0;
        };
        if coverage.len() < expected {
            log::error!("Coverage buffer too small: {} < {expected}", coverage.len());
            return 0;
        }
        let handle = self.next_coverage_handle;
        self.next_coverage_handle += 1;

        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("coverage tile a8"),
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
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("coverage bind a8"),
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
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &coverage[..expected],
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
        let bytes = checked_image_bytes(w, h, 1).unwrap_or(u64::MAX);
        self.image_bytes_total = self.image_bytes_total.saturating_add(bytes);
        self.coverages.insert(
            handle,
            CoverageTex {
                tex,
                bind,
                w,
                h,
                last_used_frame: self.frame_index,
                bytes,
            },
        );
        self.evict_budget_excess();
        handle
    }

    /// Remove a coverage tile registered with [`register_coverage_a8`](Self::register_coverage_a8).
    pub fn remove_coverage(&mut self, handle: u64) {
        if let Some(tile) = self.coverages.remove(&handle) {
            self.image_bytes_total = self.image_bytes_total.saturating_sub(tile.bytes);
        }
    }

    /// Tile dimensions, marking the handle used (keeps it alive under the
    /// eviction policy). Returns `None` for unknown handles.
    pub fn coverage_dimensions(&mut self, handle: u64) -> Option<(u32, u32)> {
        if let Some(tile) = self.coverages.get_mut(&handle) {
            tile.last_used_frame = self.frame_index;
            return Some((tile.w, tile.h));
        }
        None
    }

    fn evict_retained_excess(&mut self) {
        let mut candidates: Vec<(u64, u64, u64)> = self
            .retained
            .iter()
            .map(|(handle, image)| (*handle, image.last_used_frame, image.rgba.len() as u64))
            .collect();
        candidates.sort_by_key(|candidate| candidate.1);
        for (handle, _last_used, bytes) in candidates {
            if self.retained.len() <= MAX_RETAINED_IMAGES
                && self.retained_bytes_total <= MAX_RETAINED_IMAGE_BYTES
            {
                break;
            }
            if let Some(image) = self.retained.remove(&handle) {
                self.retained_bytes_total = self
                    .retained_bytes_total
                    .saturating_sub(bytes.min(image.rgba.len() as u64));
            }
        }
    }

    fn evict_unused_images(&mut self) {
        let now = self.frame_index;
        let evict_after = self.image_evict_after_frames;

        // Time based eviction. Eviction only frees GPU memory: retained RGBA
        // sources stay so the image can be lazily re-uploaded when drawn again.
        let mut to_evict = Vec::new();
        for (h, t) in self.images.iter() {
            if !t.evictable() {
                continue;
            }
            let last = match t {
                ImageTex::Rgba {
                    last_used_frame, ..
                } => *last_used_frame,
                ImageTex::User {
                    last_used_frame, ..
                } => *last_used_frame,
                ImageTex::Nv12 {
                    last_used_frame, ..
                } => *last_used_frame,
            };
            if now.saturating_sub(last) > evict_after {
                to_evict.push(*h);
            }
        }
        for h in to_evict {
            if self.retained.contains_key(&h) {
                self.evict_image_gpu(h);
            } else {
                self.remove_image(h);
            }
        }

        // Coverage tiles have no retained CPU copies: age-out removes them.
        let mut stale = Vec::new();
        for (h, t) in self.coverages.iter() {
            if now.saturating_sub(t.last_used_frame) > evict_after {
                stale.push(*h);
            }
        }
        for h in stale {
            self.remove_coverage(h);
        }

        self.evict_budget_excess();
    }

    fn gpu_bytes_total(&self) -> u64 {
        self.image_bytes_total
            .saturating_add(self.layer_bytes_total)
            .saturating_add(self.blend_snapshot_bytes_total)
            .saturating_add(self.working_space_bytes)
            .saturating_add(self.surface_resolve_bytes)
            .saturating_add(self.msaa_bytes)
            .saturating_add(self.ws_msaa_bytes)
            .saturating_add(self.depth_stencil_bytes)
    }

    fn budget_exceeded(&self) -> bool {
        self.image_bytes_total > self.image_budget_bytes
            || self.gpu_bytes_total() > self.gpu_budget_bytes
    }

    fn evict_budget_excess(&mut self) {
        let image_over_budget = self.image_bytes_total > self.image_budget_bytes;
        let gpu_over_budget = self.gpu_bytes_total() > self.gpu_budget_bytes;
        if !image_over_budget && !gpu_over_budget {
            return;
        }
        let mut coverage_candidates: Vec<(u64, u64)> = self
            .coverages
            .iter()
            .map(|(handle, tile)| (*handle, tile.last_used_frame))
            .collect();
        coverage_candidates.sort_by_key(|candidate| candidate.1);
        let now = self.frame_index;
        for (handle, last_used) in coverage_candidates {
            if !self.budget_exceeded() {
                break;
            }
            if last_used == now {
                continue;
            }
            self.remove_coverage(handle);
        }
        if !self.budget_exceeded() {
            return;
        }
        // Collect (handle, last_used, bytes)
        let mut candidates: Vec<(u64, u64, u64)> = self
            .images
            .iter()
            .filter_map(|(h, t)| {
                if !t.evictable() {
                    return None;
                }
                let (last, bytes) = match t {
                    ImageTex::Rgba {
                        last_used_frame,
                        bytes,
                        ..
                    } => (*last_used_frame, *bytes),
                    ImageTex::User {
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
                Some((*h, last, bytes))
            })
            .collect();

        // Sort by last_used ascending (LRU first)
        candidates.sort_by_key(|k| k.1);

        for (h, last, _bytes) in candidates {
            if !self.budget_exceeded() {
                break;
            }
            if last == now {
                continue;
            }
            if self.retained.contains_key(&h) {
                self.evict_image_gpu(h);
            } else {
                self.remove_image(h);
            }
        }
        if self.gpu_bytes_total() > self.gpu_budget_bytes {
            log::warn!(
                "renderer GPU resource budget exceeded: {} > {}",
                self.gpu_bytes_total(),
                self.gpu_budget_bytes
            );
        }
    }

    /// Set pixels per point (DPI scale) for callback `ScreenDescriptor` / `PaintCallbackInfo`.
    pub fn set_pixels_per_point(&mut self, ppp: f32) {
        if ppp.is_finite() {
            self.pixels_per_point = ppp.clamp(0.5, 8.0);
        }
    }

    /// Enable or disable linear working-space rendering.
    /// When enabled, the scene is rendered into an Rgba16Float intermediate
    /// and a final full-screen pass applies the display OETF.
    pub fn set_working_space(&mut self, enabled: bool) {
        if enabled == self.working_space {
            return;
        }
        self.working_space = enabled;
        if enabled {
            self.ensure_display_pipeline();
            self.recreate_msaa_and_depth_stencil();
            self.recreate_working_space_texture();
        } else {
            self.ws_tex = None;
            self.ws_view = None;
            self.ws_bind = None;
            self.ws_msaa_tex = None;
            self.ws_msaa_view = None;
            self.surface_resolve_tex = None;
            self.surface_resolve_view = None;
            self.surface_resolve_bytes = 0;
            self.working_space_bytes = 0;
            self.recreate_msaa_and_depth_stencil();
        }
        self.evict_budget_excess();
    }

    fn ensure_display_pipeline(&mut self) {
        if self.display_pipeline.is_some() {
            return;
        }

        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("display transform layout"),
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
        self.display_layout = Some(layout);

        let display_source: Cow<'static, str> = if self.output_format.is_srgb() {
            Cow::Borrowed(include_str!("shaders/display_transform_passthrough.wgsl"))
        } else {
            Cow::Borrowed(include_str!("shaders/display_transform.wgsl"))
        };
        let shader = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("display transform"),
                source: wgpu::ShaderSource::Wgsl(display_source),
            });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("display transform pipeline layout"),
                bind_group_layouts: &[None, self.display_layout.as_ref()],
                immediate_size: 0,
            });

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("display transform pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.output_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        self.display_pipeline = Some(pipeline);
    }

    /// Resize the render target dimensions.
    ///
    /// Recreates MSAA, depth-stencil, and working-space textures to match the
    /// new size..
    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.device.limits().max_texture_dimension_2d;
        self.output_width = if width == 0 { 0 } else { width.min(max) };
        self.output_height = if height == 0 { 0 } else { height.min(max) };
        self.recreate_msaa_and_depth_stencil();
        self.recreate_working_space_texture();
        self.evict_budget_excess();
    }

    fn recreate_working_space_texture(&mut self) {
        if !self.working_space {
            return;
        }
        self.working_space_bytes = 0;
        let w = self
            .output_width
            .clamp(1, self.device.limits().max_texture_dimension_2d);
        let h = self
            .output_height
            .clamp(1, self.device.limits().max_texture_dimension_2d);

        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("working space"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());

        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("working space bind"),
            layout: self.display_layout.as_ref().unwrap(),
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

        self.working_space_bytes =
            texture_storage_bytes(wgpu::TextureFormat::Rgba16Float, w, h).unwrap_or(0);
        self.ws_tex = Some(tex);
        self.ws_view = Some(view);
        self.ws_bind = Some(bind);
    }

    fn recreate_msaa_and_depth_stencil(&mut self) {
        self.msaa_bytes = 0;
        self.ws_msaa_bytes = 0;
        self.depth_stencil_bytes = 0;
        if self.msaa_samples > 1 && !self.working_space {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa color"),
                size: wgpu::Extent3d {
                    width: self.output_width.max(1),
                    height: self.output_height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.msaa_samples,
                dimension: wgpu::TextureDimension::D2,
                format: self.output_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.msaa_bytes = texture_storage_bytes_with_samples(
                self.output_format,
                self.output_width.max(1),
                self.output_height.max(1),
                self.msaa_samples,
            )
            .unwrap_or(0);
            self.msaa_tex = Some(tex);
            self.msaa_view = Some(view);
        } else {
            self.msaa_tex = None;
            self.msaa_view = None;
        }
        if self.working_space && self.working_space_msaa_samples > 1 {
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("working space msaa color"),
                size: wgpu::Extent3d {
                    width: self.output_width.max(1),
                    height: self.output_height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.working_space_msaa_samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.ws_msaa_bytes = texture_storage_bytes_with_samples(
                wgpu::TextureFormat::Rgba16Float,
                self.output_width.max(1),
                self.output_height.max(1),
                self.working_space_msaa_samples,
            )
            .unwrap_or(0);
            self.ws_msaa_tex = Some(tex);
            self.ws_msaa_view = Some(view);
        } else {
            self.ws_msaa_tex = None;
            self.ws_msaa_view = None;
        }
        self.surface_resolve_tex = None;
        self.surface_resolve_view = None;
        self.surface_resolve_bytes = 0;
        if self.msaa_samples > 1 && !self.working_space {
            let width = self.output_width.max(1);
            let height = self.output_height.max(1);
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("surface resolve"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.output_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.surface_resolve_bytes =
                texture_storage_bytes(self.output_format, width, height).unwrap_or(0);
            self.surface_resolve_tex = Some(tex);
            self.surface_resolve_view = Some(view);
        }

        self.depth_stencil_bytes = texture_storage_bytes_with_samples(
            wgpu::TextureFormat::Depth24PlusStencil8,
            self.output_width.max(1),
            self.output_height.max(1),
            self.active_surface_msaa_samples(),
        )
        .unwrap_or(0);
        self.depth_stencil_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-stencil (stencil clips)"),
            size: wgpu::Extent3d {
                width: self.output_width.max(1),
                height: self.output_height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.active_surface_msaa_samples(),
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_stencil_view = self
            .depth_stencil_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
    }

    fn active_surface_msaa_samples(&self) -> u32 {
        if self.working_space {
            self.working_space_msaa_samples
        } else {
            self.msaa_samples
        }
    }

    fn layer_target_format(&self) -> wgpu::TextureFormat {
        if self.working_space {
            wgpu::TextureFormat::Rgba16Float
        } else {
            self.output_format
        }
    }

    fn get_or_create_layer(
        &mut self,
        layer_id: u32,
        width: u32,
        height: u32,
        rect: repose_core::Rect,
    ) -> bool {
        if width == 0
            || height == 0
            || width > self.device.limits().max_texture_dimension_2d
            || height > self.device.limits().max_texture_dimension_2d
        {
            return false;
        }
        let needs_alloc = match self.layer_pool.get(&layer_id) {
            Some(lt) => {
                lt.width != width || lt.height != height || lt.format != self.layer_target_format()
            }
            None => true,
        };
        if !needs_alloc {
            if let Some(lt) = self.layer_pool.get_mut(&layer_id) {
                lt.rect_px = (rect.x, rect.y, rect.w, rect.h);
            }
            return true;
        }
        let color_bytes =
            texture_storage_bytes(self.layer_target_format(), width.max(1), height.max(1))
                .unwrap_or(MAX_GRAPHICS_LAYER_BYTES + 1);
        let depth_bytes = checked_image_bytes(width.max(1), height.max(1), 4)
            .unwrap_or(MAX_GRAPHICS_LAYER_BYTES + 1);
        let bytes = color_bytes.saturating_add(depth_bytes);
        let old_bytes = self
            .layer_pool
            .get(&layer_id)
            .map_or(0, |layer| layer.bytes);
        let count = self.layer_pool.len() + usize::from(!self.layer_pool.contains_key(&layer_id));
        let total = self
            .layer_bytes_total
            .saturating_sub(old_bytes)
            .saturating_add(bytes);
        let projected_gpu = self
            .gpu_bytes_total()
            .saturating_sub(old_bytes)
            .checked_add(bytes)
            .unwrap_or(u64::MAX);
        if count > MAX_GRAPHICS_LAYERS
            || bytes > MAX_GRAPHICS_LAYER_BYTES
            || total > MAX_GRAPHICS_LAYER_BYTES
            || projected_gpu > self.gpu_budget_bytes
        {
            log::warn!("graphics layer budget exhausted; layer {layer_id} skipped");
            if let Some(old) = self.layer_pool.remove(&layer_id) {
                self.layer_bytes_total = self.layer_bytes_total.saturating_sub(old.bytes);
            }
            return false;
        }
        if let Some(old) = self.layer_pool.remove(&layer_id) {
            self.layer_bytes_total = self.layer_bytes_total.saturating_sub(old.bytes);
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
            format: self.layer_target_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
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
                    resource: wgpu::BindingResource::Sampler(&self.layer_sampler),
                },
            ],
        });
        let bind_linear = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("layer bind linear"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.layer_sampler_linear),
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
                bind_linear,
                depth_stencil_view,
                width,
                height,
                format: self.layer_target_format(),
                bytes,
                rect_px: (rect.x, rect.y, rect.w, rect.h),
            },
        );
        self.layer_bytes_total = self.layer_bytes_total.saturating_add(bytes);
        true
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

    fn upload_glyph_mask(&mut self, key: repose_text::GlyphKey, px: f32) -> Option<GlyphInfo> {
        let keyp = (key, px.to_bits());
        if let Some(info) = self.atlas_mask.map.get(&keyp) {
            return Some(*info);
        }

        let gb = repose_text::rasterize(key, px)?;
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
        };
        self.atlas_mask.map.insert(keyp, info);
        Some(info)
    }

    fn upload_glyph_color(&mut self, key: repose_text::GlyphKey, px: f32) -> Option<GlyphInfo> {
        let keyp = (key, px.to_bits());
        if let Some(info) = self.atlas_color.map.get(&keyp) {
            return Some(*info);
        }
        let gb = repose_text::rasterize(key, px)?;
        if !matches!(gb.content, repose_text::SwashContent::Color) {
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
        };
        self.atlas_color.map.insert(keyp, info);
        Some(info)
    }

    fn alloc_space_mask(&mut self, w: u32, h: u32) -> bool {
        let Some(x_end) = self
            .atlas_mask
            .next_x
            .checked_add(w)
            .and_then(|v| v.checked_add(1))
        else {
            return false;
        };
        if x_end >= self.atlas_mask.size {
            self.atlas_mask.next_x = 1;
            let Some(next_y) = self
                .atlas_mask
                .next_y
                .checked_add(self.atlas_mask.row_h)
                .and_then(|v| v.checked_add(1))
            else {
                return false;
            };
            self.atlas_mask.next_y = next_y;
            self.atlas_mask.row_h = 0;
        }
        let Some(y_end) = self
            .atlas_mask
            .next_y
            .checked_add(h)
            .and_then(|v| v.checked_add(1))
        else {
            return false;
        };
        y_end < self.atlas_mask.size
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
        for (k, px_bits) in keys {
            let _ = self.upload_glyph_mask(k, f32::from_bits(px_bits));
        }
    }

    fn alloc_space_color(&mut self, w: u32, h: u32) -> bool {
        let Some(x_end) = self
            .atlas_color
            .next_x
            .checked_add(w)
            .and_then(|v| v.checked_add(1))
        else {
            return false;
        };
        if x_end >= self.atlas_color.size {
            self.atlas_color.next_x = 1;
            let Some(next_y) = self
                .atlas_color
                .next_y
                .checked_add(self.atlas_color.row_h)
                .and_then(|v| v.checked_add(1))
            else {
                return false;
            };
            self.atlas_color.next_y = next_y;
            self.atlas_color.row_h = 0;
        }
        let Some(y_end) = self
            .atlas_color
            .next_y
            .checked_add(h)
            .and_then(|v| v.checked_add(1))
        else {
            return false;
        };
        y_end < self.atlas_color.size
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
        for (k, px_bits) in keys {
            let _ = self.upload_glyph_color(k, f32::from_bits(px_bits));
        }
    }
}

/// Packed brush fields shared by the shape instances (border, ellipse,
/// ellipse border, arc). Gradient endpoints are shape-local px; the shaders
/// recenter `(0,0)` at the shape top-left. Radial packs `center` into
/// `grad_p0` and `radius` into `grad_p1.x`; sweep packs `center` into
/// `grad_p0`.
///
/// `rect` is the shape's scene-space bounds and `transform` the accumulated
/// scene transform.
/// Endpoints are converted from shape-local px through the inverse linear
/// part so rotation and uniform scale cancel against the shader's
/// un-rotation. Non-uniform scale and shear distort the gradient the same
/// way they distort the shape (the shader un-rotates but cannot un-scale
/// pixels).
fn brush_to_shape_fields(
    brush: &Brush,
    rect: &repose_core::Rect,
    transform: &Transform,
) -> (u32, u32, [f32; 4], [f32; 4], [f32; 2], [f32; 2], u32) {
    let to_local = |p: Vec2| {
        let m = transform.linear();
        let det = m[0] * m[3] - m[1] * m[2];
        if det.abs() < 1e-12 {
            return [p.x, p.y];
        }
        [
            (m[3] * p.x - m[1] * p.y) / det,
            (-m[2] * p.x + m[0] * p.y) / det,
        ]
    };
    match brush {
        Brush::Solid(c) => (
            0u32,
            0u32,
            c.to_linear(),
            [0.0; 4],
            [0.0; 2],
            [0.0; 2],
            0u32,
        ),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => (
            1u32,
            0u32,
            start_color.to_linear(),
            end_color.to_linear(),
            to_local(*start),
            to_local(*end),
            0u32,
        ),
        Brush::LinearNormalized {
            start,
            end,
            start_color,
            end_color,
        } => (
            1u32,
            0u32,
            start_color.to_linear(),
            end_color.to_linear(),
            to_local(Vec2 {
                x: start.x * rect.w,
                y: start.y * rect.h,
            }),
            to_local(Vec2 {
                x: end.x * rect.w,
                y: end.y * rect.h,
            }),
            0u32,
        ),
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => (
            1u32,
            1u32,
            start_color.to_linear(),
            end_color.to_linear(),
            to_local(*center),
            [radius.max(0.0), 0.0],
            0u32,
        ),
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => (
            1u32,
            2u32,
            start_color.to_linear(),
            end_color.to_linear(),
            to_local(*center),
            [0.0, 0.0],
            0u32,
        ),
        _ => (0u32, 0u32, [0.0; 4], [0.0; 4], [0.0; 2], [0.0; 2], 0u32),
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
        Brush::LinearNormalized {
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
        Brush::Radial { start_color, .. } => (
            0u32,
            start_color.to_linear(),
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
        ),
        Brush::Sweep { start_color, .. } => (
            0u32,
            start_color.to_linear(),
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
        ),
        _ => (0u32, [0.0; 4], [0.0; 4], [0.0; 2], [0.0; 2]),
    }
}

/// Fallback color when a [`Brush`] reaches a solid-only path (glyph atlas
/// uploads for gradient text). Uses the gradient's start color.
#[allow(dead_code)]
fn brush_to_solid_color(brush: &Brush) -> [f32; 4] {
    match brush {
        Brush::Solid(c) => c.to_linear(),
        Brush::Linear { start_color, .. } => start_color.to_linear(),
        Brush::LinearNormalized { start_color, .. } => start_color.to_linear(),
        Brush::Radial { start_color, .. } => start_color.to_linear(),
        Brush::Sweep { start_color, .. } => start_color.to_linear(),
        _ => [0.0; 4],
    }
}

fn init_atlas_mask(device: &wgpu::Device) -> AtlasA8 {
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

    AtlasA8 {
        tex,
        view,
        sampler,
        size,
        next_x: 1,
        next_y: 1,
        row_h: 0,
        map: HashMap::new(),
    }
}

fn init_atlas_color(device: &wgpu::Device) -> AtlasRGBA {
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
    AtlasRGBA {
        tex,
        view,
        sampler,
        size,
        next_x: 1,
        next_y: 1,
        row_h: 0,
        map: HashMap::new(),
    }
}

#[cfg(feature = "winit-surface")]
impl WgpuSurfaceBackend {
    /// Drop the surface, keeping device/queue/pipelines. Releases the old
    /// native-window binding. Call `recreate_surface` to present again.
    pub fn take_surface(&mut self) -> Option<wgpu::Surface<'static>> {
        self.surface.take()
    }

    /// Create a fresh surface for `window` on the retained instance and
    /// configure it. Recovers from `CurrentSurfaceTexture::Lost`, where
    /// reconfiguring the old surface object cannot help.
    pub fn recreate_surface(&mut self, window: &Arc<winit::window::Window>) -> anyhow::Result<()> {
        self.window = Some(window.clone());
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            self.surface = None;
            self.pending_reconfigure = false;
            self.renderer.resize(0, 0);
            anyhow::bail!("window has zero size; surface recreation deferred");
        }
        let Some(instance) = self.instance.as_ref() else {
            anyhow::bail!("no wgpu instance retained; cannot recreate surface")
        };
        let Some(config) = self.surface_config.as_mut() else {
            anyhow::bail!("no surface config retained; cannot recreate surface")
        };
        let max = self.renderer.device.limits().max_texture_dimension_2d;
        let width = size.width.min(max);
        let height = size.height.min(max);
        config.width = width;
        config.height = height;
        let surface = instance.create_surface(window.clone())?;
        surface.configure(&self.renderer.device, config);
        self.surface = Some(surface);
        self.pending_reconfigure = false;
        self.renderer.resize(width, height);
        Ok(())
    }
}

#[cfg(feature = "winit-surface")]
impl RenderBackend for WgpuSurfaceBackend {
    fn configure_surface(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            self.renderer.resize(0, 0);
            self.pending_reconfigure = true;
            return;
        }
        let max = self.renderer.device.limits().max_texture_dimension_2d;
        let width = width.min(max);
        let height = height.min(max);
        if self.renderer.output_width == width && self.renderer.output_height == height {
            if let Some(ref mut config) = self.surface_config {
                config.width = width;
                config.height = height;
            }
            return;
        }
        if let Some(ref mut config) = self.surface_config {
            config.width = width;
            config.height = height;
        }
        self.renderer.resize(width, height);
        if let (Some(surface), Some(config)) = (self.surface.as_ref(), self.surface_config.as_ref())
        {
            surface.configure(&self.renderer.device, config);
        }
    }

    fn frame(&mut self, scene: &Scene, _glyph_cfg: GlyphRasterConfig) -> bool {
        if self.pending_reconfigure {
            if let (Some(surface), Some(config)) =
                (self.surface.as_ref(), self.surface_config.as_ref())
            {
                surface.configure(&self.renderer.device, config);
            }
            self.pending_reconfigure = false;
        }

        if self.renderer.output_width == 0 || self.renderer.output_height == 0 {
            request_frame();
            return false;
        }
        if let Err(error) = self.renderer.begin_frame() {
            log::warn!("begin renderer frame: {error:#}");
            request_frame();
            return false;
        }

        let Some(surface) = self.surface.as_ref() else {
            self.renderer.end_frame();
            request_frame();
            return false;
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                self.pending_reconfigure = true;
                f
            }
            other @ (wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
                self.surface = None;
                let result = self
                    .window
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no window retained for surface recreation"))
                    .and_then(|window| self.recreate_surface(&window));
                if let Err(error) = result {
                    log::warn!("surface {other:?}; recreation deferred: {error:#}");
                }
                self.renderer.end_frame();
                request_frame();
                return false;
            }
            other => {
                match other {
                    wgpu::CurrentSurfaceTexture::Validation => {
                        log::warn!("surface {other:?}; reconfiguring next frame");
                        self.pending_reconfigure = true;
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded => {
                        log::debug!("surface {other:?}; retrying next frame");
                    }
                    _ => {}
                }
                self.renderer.end_frame();
                request_frame();
                return false;
            }
        };

        let swap_view = if let Some(view_format) = self
            .surface_config
            .as_ref()
            .and_then(|c| c.view_formats.iter().find(|f| f.is_srgb()).copied())
        {
            frame.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(view_format),
                ..Default::default()
            })
        } else {
            frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let mut encoder =
            self.renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame encoder"),
                });

        self.renderer.render_scene_to_encoder_with_texture(
            scene,
            &mut encoder,
            &swap_view,
            Some(&frame.texture),
            None,
        );

        //NOTE: The WebGL HAL present path (fullscreen triangle / blit) does not
        // restore gl.colorMask. Hence this is needed to prevent frames from going transparent.
        {
            let _reset = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webgl color_mask reset before present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        self.renderer
            .queue
            .submit(std::iter::once(encoder.finish()));
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| self.renderer.queue.present(frame))) {
            log::warn!("queue.present panicked: {:?}", e);
            self.renderer.end_frame();
            request_frame();
            return false;
        }
        self.renderer.end_frame();
        true
    }
}

impl WgpuSceneRenderer {
    /// Open a translator-owned flatten layer for a perspective `PushTransform`.
    ///
    /// True perspective cannot ride the affine instance fast path, so the
    /// subtree renders flat into an offscreen layer and is composited back
    /// projectively on the matching pop (CSS-style flattening). The layer
    /// rect is the currently visible scissor in this target: content outside
    /// it is invisible in the parent, so clipping it in the layer changes
    /// nothing. Children keep the node's affine part on the stack (so
    /// `combine` stays affine-only) plus a layer-local shift, exactly like
    /// producer-owned blur layers — which is exact under rigid ancestors
    /// (translations commute) and the documented layer contract otherwise.
    #[allow(clippy::too_many_arguments)]
    fn replay_active_clips(
        &mut self,
        clips: &[ActiveClip],
        origin: (f32, f32),
        target_size: (f32, f32),
        pass: &mut Pass,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Vec<ActiveClip> {
        let mut current = repose_core::Rect {
            x: 0.0,
            y: 0.0,
            w: target_size.0,
            h: target_size.1,
        };
        let mut replayed = Vec::with_capacity(clips.len());
        for (index, clip) in clips.iter().enumerate() {
            let has_inverse = clips[..index].iter().any(ActiveClip::difference);
            let mut blocked = clip.blocked();
            if has_inverse && !blocked {
                log::error!(
                    "unsupported clip nesting: a Difference clip cannot contain another clip"
                );
                blocked = true;
            }
            let difference = clip.difference();
            let mut command = None;
            match clip {
                ActiveClip::Rect {
                    off: _,
                    cnt: _,
                    rect,
                    radii,
                    ..
                } => {
                    let local_rect = translated_rect(*rect, origin.0, origin.1);
                    let next = if difference {
                        current
                    } else {
                        intersect(current, local_rect)
                    };
                    let scissor = rect_to_scissor(
                        next,
                        target_size.0.max(1.0) as u32,
                        target_size.1.max(1.0) as u32,
                    );
                    let has_area = local_rect.x.is_finite()
                        && local_rect.y.is_finite()
                        && local_rect.w.is_finite()
                        && local_rect.h.is_finite()
                        && local_rect.w > 0.0
                        && local_rect.h > 0.0;
                    if blocked || !has_area || (!difference && scissor.2 == 0) {
                        blocked = blocked || !difference;
                    } else {
                        let ndc = rect_to_ndc(local_rect, target_size.0, target_size.1);
                        let instance = ClipInstance {
                            xywh: [ndc[0] + ndc[2] * 0.5, ndc[1] + ndc[3] * 0.5, ndc[2], ndc[3]],
                            radii: *radii,
                            fwd_mat: [1.0, 0.0, 0.0, 1.0],
                        };
                        let bytes = bytemuck::bytes_of(&instance);
                        if self
                            .clip_ring
                            .grow_to_fit(
                                &self.device,
                                encoder,
                                u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                            )
                            .is_ok()
                            && let Ok(new_off) = self.clip_ring.alloc_write(&self.queue, bytes)
                        {
                            command = Some(Cmd::ClipPush {
                                off: new_off,
                                cnt: 1,
                                scissor,
                                difference,
                                applied: true,
                            });
                            replayed.push(ActiveClip::Rect {
                                off: new_off,
                                cnt: 1,
                                rect: *rect,
                                radii: *radii,
                                difference,
                                applied: true,
                                blocked: false,
                            });
                        } else {
                            blocked = blocked || !difference;
                        }
                    }
                    current = next;
                }
                ActiveClip::Vector {
                    voff,
                    vcnt,
                    ioff,
                    icnt,
                    mesh,
                    affine,
                    ..
                } => {
                    let mut local_affine = *affine;
                    local_affine[2] -= origin.0;
                    local_affine[5] -= origin.1;
                    let aabb = mesh_aabb(mesh, local_affine);
                    let next = if difference {
                        current
                    } else {
                        intersect(current, aabb)
                    };
                    let scissor = rect_to_scissor(
                        next,
                        target_size.0.max(1.0) as u32,
                        target_size.1.max(1.0) as u32,
                    );
                    let has_area = aabb.x.is_finite()
                        && aabb.y.is_finite()
                        && aabb.w.is_finite()
                        && aabb.h.is_finite()
                        && aabb.w > 0.0
                        && aabb.h > 0.0
                        && *vcnt > 0
                        && *icnt > 0;
                    if blocked || !has_area || (!difference && scissor.2 == 0) {
                        blocked = blocked || !difference;
                    } else if let Some(uoff) = self.alloc_mesh_uniform(mesh_uniform_from_paint(
                        local_affine,
                        &repose_core::PaintDesc::Solid,
                    )) {
                        command = Some(Cmd::VectorClipPush {
                            voff: *voff,
                            vcnt: *vcnt,
                            ioff: *ioff,
                            icnt: *icnt,
                            uoff,
                            scissor,
                            difference,
                            applied: true,
                        });
                        replayed.push(ActiveClip::Vector {
                            voff: *voff,
                            vcnt: *vcnt,
                            ioff: *ioff,
                            icnt: *icnt,
                            uoff,
                            mesh: mesh.clone(),
                            affine: *affine,
                            difference,
                            applied: true,
                            blocked: false,
                        });
                    } else {
                        blocked = blocked || !difference;
                    }
                    current = next;
                }
            }
            if blocked {
                current = repose_core::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                };
            }
            if command.is_none() {
                replayed.push(match clip {
                    ActiveClip::Rect {
                        rect,
                        radii,
                        difference,
                        ..
                    } => ActiveClip::Rect {
                        off: 0,
                        cnt: 0,
                        rect: *rect,
                        radii: *radii,
                        difference: *difference,
                        applied: false,
                        blocked,
                    },
                    ActiveClip::Vector {
                        voff,
                        vcnt,
                        ioff,
                        icnt,
                        mesh,
                        affine,
                        difference,
                        ..
                    } => ActiveClip::Vector {
                        voff: *voff,
                        vcnt: *vcnt,
                        ioff: *ioff,
                        icnt: *icnt,
                        uoff: 0,
                        mesh: mesh.clone(),
                        affine: *affine,
                        difference: *difference,
                        applied: false,
                        blocked,
                    },
                });
            }
            if let Some(command) = command {
                pass.cmds.push(command);
            }
            pass.active_scissor = if blocked {
                None
            } else {
                let scissor = rect_to_scissor(
                    current,
                    target_size.0.max(1.0) as u32,
                    target_size.1.max(1.0) as u32,
                );
                (scissor.2 > 0 && scissor.3 > 0).then_some(scissor)
            };
        }
        replayed
    }

    #[allow(clippy::too_many_arguments)]
    fn push_perspective_layer(
        &mut self,
        node: Transform,
        top: Transform,
        transform_stack: &mut Vec<Transform>,
        scissor_stack: &mut Vec<repose_core::Rect>,
        root_clip_rect: &mut repose_core::Rect,
        current_target_size: &mut (f32, f32),
        current_pass: &mut Pass,
        active_clips: &mut Vec<ActiveClip>,
        passes: &mut Vec<Pass>,
        target_stack: &mut Vec<PassTarget>,
        flatten_stack: &mut Vec<FlattenRecord>,
        id_head: &mut u32,
        pass_id_head: &mut u64,
        ids_used: &mut Vec<u32>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // Full projective map: affine ancestors over the node's map.
        // Ancestors are affine by construction (perspective always flattens
        // at push, and only stripped affines reach the stack).
        let map =
            Transform::compose_projective(&top.projective_matrix(), &node.projective_matrix());
        let scr = scissor_stack.last().copied().unwrap_or(*root_clip_rect);
        let max_dimension = self.device.limits().max_texture_dimension_2d as f32;
        if !scr.w.is_finite()
            || !scr.h.is_finite()
            || scr.w < 1.0
            || scr.h < 1.0
            || scr.w > max_dimension
            || scr.h > max_dimension
        {
            return;
        }
        let w = scr.w.ceil();
        let h = scr.h.ceil();
        let layer_rect = repose_core::Rect {
            x: scr.x,
            y: scr.y,
            w,
            h,
        };
        // Translator-owned ids live far above producer ids (which start at 1
        // per scene) and are drained from the pool after each frame.
        let layer_id = *id_head;
        *id_head = id_head.wrapping_add(1);
        ids_used.push(layer_id);

        let stack_len = transform_stack.len();
        // Children render with the ancestors' map only: the node's own
        // affine part lives in `map` and applies once, at composite time.
        // (Pushing the stripped affine here too would foreshorten twice.)
        transform_stack.push(top);
        transform_stack.push(Transform::translate(-layer_rect.x, -layer_rect.y));

        let saved_scissor_stack = std::mem::replace(
            scissor_stack,
            vec![repose_core::Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            }],
        );
        let saved_root = std::mem::replace(
            root_clip_rect,
            repose_core::Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
        );
        let saved_size = std::mem::replace(current_target_size, (w, h));
        let prev_target = current_pass.target;
        let saved_active_scissor = current_pass.active_scissor;
        let saved_clips = std::mem::take(active_clips);
        let layer_pass_id = *pass_id_head;
        *pass_id_head += 1;
        let mut layer_pass = Pass {
            id: layer_pass_id,
            target: PassTarget::Layer(layer_id),
            initial_scissor: (0, 0, w as u32, h as u32),
            active_scissor: Some((0, 0, w as u32, h as u32)),
            clear_color: Some([0.0, 0.0, 0.0, 0.0]),
            cmds: Vec::new(),
        };
        let layer_clips = self.replay_active_clips(
            &saved_clips,
            (layer_rect.x, layer_rect.y),
            (w, h),
            &mut layer_pass,
            encoder,
        );
        *active_clips = layer_clips;
        let saved = std::mem::replace(current_pass, layer_pass);
        passes.push(saved);
        target_stack.push(prev_target);
        self.get_or_create_layer(layer_id, w as u32, h as u32, layer_rect);
        *current_target_size = (w, h);
        flatten_stack.push(FlattenRecord {
            stack_len,
            layer_id,
            map,
            layer_rect,
            saved_scissor_stack,
            saved_root,
            saved_size,
            saved_active_scissor,
            saved_clips,
        });
    }

    /// Close a flatten layer: restore the parent target and composite the
    /// layer texture through the recorded projective map.
    #[allow(clippy::too_many_arguments)]
    fn pop_perspective_layer(
        &mut self,
        rec: FlattenRecord,
        scissor_stack: &mut Vec<repose_core::Rect>,
        root_clip_rect: &mut repose_core::Rect,
        current_target_size: &mut (f32, f32),
        current_pass: &mut Pass,
        active_clips: &mut Vec<ActiveClip>,
        passes: &mut Vec<Pass>,
        target_stack: &mut Vec<PassTarget>,
        pass_id_head: &mut u64,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        *scissor_stack = rec.saved_scissor_stack;
        *root_clip_rect = rec.saved_root;
        *current_target_size = rec.saved_size;
        *active_clips = rec.saved_clips;
        let resumed_pass_id = *pass_id_head;
        *pass_id_head += 1;
        let saved = std::mem::replace(
            current_pass,
            Pass {
                id: resumed_pass_id,
                target: target_stack.pop().unwrap_or(PassTarget::Surface),
                initial_scissor: rec.saved_active_scissor.unwrap_or((0, 0, 1, 1)),
                active_scissor: rec.saved_active_scissor,
                clear_color: None,
                cmds: Vec::new(),
            },
        );
        passes.push(saved);

        // Project the layer-rect corners (parent space) to NDC in the
        // resumed (parent) target, keeping each corner's homogeneous w for
        // perspective-correct sampling.
        let (tw, th) = rec.saved_size;
        let r = rec.layer_rect;
        let corners = [
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
        ];
        let mut ndc = [[0.0f32; 2]; 4];
        let mut ws = [1.0f32; 4];
        let mut all_behind = true;
        for (i, (x, y)) in corners.iter().enumerate() {
            let w_raw = rec.map[6] * x + rec.map[7] * y + rec.map[8];
            let w = if w_raw.abs() < 1e-6 {
                if w_raw < 0.0 { -1e-6 } else { 1e-6 }
            } else {
                w_raw
            };
            if w > 0.0 {
                all_behind = false;
            }
            let px = (rec.map[0] * x + rec.map[1] * y + rec.map[2]) / w;
            let py = (rec.map[3] * x + rec.map[4] * y + rec.map[5]) / w;
            ndc[i] = [px / tw * 2.0 - 1.0, 1.0 - py / th * 2.0];
            ws[i] = w;
        }
        if all_behind {
            // Entire subtree behind the viewer: nothing to composite (the
            // layer pass still ran, but its output is correctly discarded).
            return;
        }
        let Some(layer) = self.layer_pool.get(&rec.layer_id) else {
            return;
        };
        let uv_u1 = layer.rect_px.2 / layer.width.max(1) as f32;
        let uv_v1 = layer.rect_px.3 / layer.height.max(1) as f32;
        let inst = ProjectiveInstance {
            c0: ndc[0],
            c1: ndc[1],
            c2: ndc[2],
            c3: ndc[3],
            uv: [0.0, 0.0, uv_u1, uv_v1],
            w: ws,
            alpha: 1.0,
            _pad: [0.0; 3],
        };
        if self
            .projective_ring
            .grow_to_fit(
                &self.device,
                encoder,
                std::mem::size_of::<ProjectiveInstance>() as u64,
            )
            .is_err()
        {
            return;
        }
        let bytes = bytemuck::bytes_of(&inst);
        let Ok(off) = self.projective_ring.alloc_write(&self.queue, bytes) else {
            return;
        };
        current_pass.cmds.push(Cmd::CompositeProjective {
            off,
            cnt: 1,
            layer_id: rec.layer_id,
        });
    }

    fn upload_mesh_geometry(
        &mut self,
        mesh: &repose_core::VectorMeshData,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<(u64, u32, u64, u32)> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            log::warn!("vector mesh has no geometry");
            return None;
        }
        if mesh.indices.len() % 3 != 0 {
            log::warn!("vector mesh index count is not a triangle list");
            return None;
        }
        let vertex_count = u32::try_from(mesh.vertices.len()).ok()?;
        let index_count = u32::try_from(mesh.indices.len()).ok()?;
        if mesh.indices.iter().any(|&index| index >= vertex_count) {
            log::warn!("vector mesh index is outside its vertex buffer");
            return None;
        }
        if mesh.vertices.iter().any(|vertex| {
            vertex.pos.iter().any(|value| !value.is_finite())
                || vertex.color.iter().any(|value| !value.is_finite())
                || vertex.uv.iter().any(|value| !value.is_finite())
        }) {
            log::warn!("vector mesh contains non-finite vertex data");
            return None;
        }
        let verts: Vec<MeshVertex> = mesh
            .vertices
            .iter()
            .map(|v| MeshVertex {
                pos: v.pos,
                color: v.color,
                uv: v.uv,
            })
            .collect();
        let vbytes = bytemuck::cast_slice(&verts);
        let vlen = u64::try_from(vbytes.len()).ok()?;
        self.mesh_verts
            .grow_to_fit(&self.device, encoder, vlen)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        let voff = self
            .mesh_verts
            .alloc_write(&self.queue, vbytes)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        let ibytes = bytemuck::cast_slice(&mesh.indices);
        let ilen = u64::try_from(ibytes.len()).ok()?;
        self.mesh_indices
            .grow_to_fit(&self.device, encoder, ilen)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        let ioff = self
            .mesh_indices
            .alloc_write(&self.queue, ibytes)
            .map_err(|error| log::error!("{error:#}"))
            .ok()?;
        Some((voff, vertex_count, ioff, index_count))
    }

    fn alloc_mesh_uniform(&mut self, u: MeshUniform) -> Option<u64> {
        let size = std::mem::size_of::<MeshUniform>() as u64;
        let next = self
            .mesh_uniform_head
            .checked_add(self.mesh_uniform_alignment)?;
        if next > self.mesh_uniform_cap || next > u64::from(u32::MAX) {
            log::warn!("mesh uniform buffer exhausted; remaining meshes skipped");
            return None;
        }
        let slot = self.mesh_uniform_head;
        if slot.checked_add(size)? > self.mesh_uniform_cap {
            return None;
        }
        self.queue
            .write_buffer(&self.mesh_uniform_buf, slot, bytemuck::bytes_of(&u));
        self.mesh_uniform_head = next;
        Some(slot)
    }

    /// Render one backdrop-dependent blend mesh: isolate the mesh into a
    /// translator-owned graphics layer, then composite it over the current
    /// target with the backdrop-blend shader. Works for surface parents and
    /// layer parents alike: the snapshot copy, isolation layer, and
    /// composite quad are all expressed in the parent target's pixel space.
    /// Callers must invoke this while the current pass targets the recorded
    /// parent: the translator never splits passes between here and the
    /// appended composite (only `BeginLayer`/perspective push new passes,
    /// and neither can intervene mid-call). The executor re-checks this
    /// (`BlendLayer.parent`) and skips a misplaced composite rather than
    /// drawing over the wrong target.
    #[allow(clippy::too_many_arguments)]
    fn emit_isolated_blend(
        &mut self,
        mesh: std::sync::Arc<repose_core::VectorMeshData>,
        transform: [f32; 6],
        paint: repose_core::PaintDesc,
        blend: repose_core::BlendMode,
        current_transform: &repose_core::Transform,
        current_pass: &mut Pass,
        active_clips: &[ActiveClip],
        passes: &mut Vec<Pass>,
        id_head: &mut u32,
        pass_id_head: &mut u64,
        ids_used: &mut Vec<u32>,
        scissor: (u32, u32, u32, u32),
        encoder: &mut wgpu::CommandEncoder,
        fb_w: f32,
        fb_h: f32,
    ) {
        if scissor.2 == 0 || scissor.3 == 0 || active_clips.iter().any(ActiveClip::blocked) {
            return;
        }
        let parent_target = current_pass.target;
        let (parent_w, parent_h) = match parent_target {
            PassTarget::Surface => (fb_w, fb_h),
            PassTarget::Layer(id) => match self.layer_pool.get(&id) {
                Some(layer) => (layer.width as f32, layer.height as f32),
                None => return,
            },
        };
        let affine = combine_mesh_affine(current_transform, transform);
        let local_rect = intersect(
            mesh_aabb(&mesh, affine),
            repose_core::Rect {
                x: 0.0,
                y: 0.0,
                w: parent_w,
                h: parent_h,
            },
        );
        if !local_rect.x.is_finite()
            || !local_rect.y.is_finite()
            || !local_rect.w.is_finite()
            || !local_rect.h.is_finite()
            || local_rect.w <= 0.0
            || local_rect.h <= 0.0
            || local_rect.w > self.device.limits().max_texture_dimension_2d as f32
            || local_rect.h > self.device.limits().max_texture_dimension_2d as f32
        {
            return;
        }
        let layer_rect = repose_core::Rect {
            x: local_rect.x.floor(),
            y: local_rect.y.floor(),
            w: (local_rect.x + local_rect.w).ceil() - local_rect.x.floor(),
            h: (local_rect.y + local_rect.h).ceil() - local_rect.y.floor(),
        };
        let w = layer_rect.w;
        let h = layer_rect.h;
        let local_rect = layer_rect;
        let layer_id = *id_head;
        *id_head = id_head.wrapping_add(1);
        ids_used.push(layer_id);

        // Snapshot the parent target region before compositing: the blend
        // shader samples it as the backdrop. The copy runs at execution
        // time, after all earlier passes have rendered.
        let snapshot_format = match parent_target {
            PassTarget::Surface if self.working_space => wgpu::TextureFormat::Rgba16Float,
            PassTarget::Surface => self.output_format,
            PassTarget::Layer(_) => self.layer_target_format(),
        };
        if !self.alloc_blend_snapshot(layer_id, w as u32, h as u32, snapshot_format) {
            log::warn!("blend snapshot budget exhausted; blend skipped");
            return;
        }
        let source_pass_id = *pass_id_head;
        *pass_id_head += 1;
        let resumed_pass_id = *pass_id_head;
        *pass_id_head += 1;
        self.blend_copies.push(BlendCopy {
            pass_id: resumed_pass_id,
            blend_id: layer_id,
            target: parent_target,
            region: layer_rect,
        });

        // Render the mesh alone into the layer (layer-local shift so the
        // layer owns exactly the mesh bbox), then resume the parent pass.
        // The composite runs in the resumed parent pass.
        let mut layer_pass = Pass {
            id: source_pass_id,
            target: PassTarget::Layer(layer_id),
            initial_scissor: (0, 0, w as u32, h as u32),
            active_scissor: Some((0, 0, w as u32, h as u32)),
            clear_color: Some([0.0, 0.0, 0.0, 0.0]),
            cmds: Vec::new(),
        };
        self.replay_active_clips(
            active_clips,
            (local_rect.x, local_rect.y),
            (w, h),
            &mut layer_pass,
            encoder,
        );
        self.get_or_create_layer(layer_id, w as u32, h as u32, layer_rect);
        let shift = repose_core::Transform::translate(-local_rect.x, -local_rect.y);
        let local = current_transform.combine(&shift);
        self.emit_vector_mesh(
            &local,
            &mesh,
            transform,
            &paint,
            repose_core::BlendMode::Alpha,
            &mut layer_pass.cmds,
            encoder,
        );
        let saved = std::mem::replace(
            current_pass,
            Pass {
                id: resumed_pass_id,
                target: parent_target,
                initial_scissor: scissor,
                active_scissor: Some(scissor),
                clear_color: None,
                cmds: Vec::new(),
            },
        );
        passes.push(layer_pass);
        passes.push(saved);

        // Composite quad over the mesh bbox in the parent target's pixel
        // space. The executor maps this NDC against `parent`, so layer
        // parents at any origin work.
        let ndc = {
            let cx = local_rect.x + local_rect.w * 0.5;
            let cy = local_rect.y + local_rect.h * 0.5;
            let ndc_cx = (cx / parent_w) * 2.0 - 1.0;
            let ndc_cy = 1.0 - (cy / parent_h) * 2.0;
            let ndc_w = (local_rect.w / parent_w) * 2.0;
            let ndc_h = (local_rect.h / parent_h) * 2.0;
            [ndc_cx, ndc_cy, ndc_w, ndc_h]
        };
        let inst = BlendInstance {
            xywh: ndc,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            fwd_mat: [1.0, 0.0, 0.0, 1.0],
            mode: blend.shader_mode(),
            _pad: [0.0; 3],
        };
        if self
            .blend_ring
            .grow_to_fit(
                &self.device,
                encoder,
                std::mem::size_of::<BlendInstance>() as u64,
            )
            .is_err()
        {
            return;
        }
        let bytes = bytemuck::bytes_of(&inst);
        let Ok(off) = self.blend_ring.alloc_write(&self.queue, bytes) else {
            return;
        };
        current_pass.cmds.push(Cmd::BlendLayer {
            off,
            cnt: 1,
            src_layer: layer_id,
            dst_layer: Some(layer_id),
            parent: parent_target,
            scissor,
        });
    }

    /// Allocate (or reuse) the backdrop snapshot texture for an isolated
    /// blend layer.
    fn alloc_blend_snapshot(
        &mut self,
        layer_id: u32,
        w: u32,
        h: u32,
        format: wgpu::TextureFormat,
    ) -> bool {
        let reuse = self
            .blend_snapshots
            .get(&layer_id)
            .is_some_and(|s| s.width == w && s.height == h);
        if reuse {
            return true;
        }
        if w == 0 || h == 0 {
            return false;
        }
        let Ok(bytes) = texture_storage_bytes(format, w, h) else {
            return false;
        };
        let old_bytes = self
            .blend_snapshots
            .get(&layer_id)
            .map_or(0, |snapshot| snapshot.bytes);
        let projected = self
            .gpu_bytes_total()
            .saturating_sub(old_bytes)
            .checked_add(bytes)
            .unwrap_or(u64::MAX);
        if bytes > MAX_BLEND_SNAPSHOT_BYTES || projected > self.gpu_budget_bytes {
            return false;
        }
        if let Some(old) = self.blend_snapshots.remove(&layer_id) {
            self.blend_snapshot_bytes_total =
                self.blend_snapshot_bytes_total.saturating_sub(old.bytes);
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blend backdrop snapshot"),
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
            label: Some("blend snapshot bind"),
            layout: &self.image_bind_layout_rgba,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.layer_sampler),
                },
            ],
        });
        self.blend_snapshot_bytes_total = self.blend_snapshot_bytes_total.saturating_add(bytes);
        self.blend_snapshots.insert(
            layer_id,
            BlendSnapshot {
                texture: tex,
                bind,
                width: w,
                height: h,
                bytes,
            },
        );
        true
    }

    fn remove_blend_snapshot(&mut self, layer_id: u32) {
        if let Some(snapshot) = self.blend_snapshots.remove(&layer_id) {
            self.blend_snapshot_bytes_total = self
                .blend_snapshot_bytes_total
                .saturating_sub(snapshot.bytes);
        }
    }

    fn blend_snapshot_texture(&self, layer_id: u32) -> Option<wgpu::Texture> {
        self.blend_snapshots
            .get(&layer_id)
            .map(|s| s.texture.clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_vector_mesh(
        &mut self,
        current_transform: &Transform,
        mesh: &repose_core::VectorMeshData,
        transform: [f32; 6],
        paint: &repose_core::PaintDesc,
        blend: repose_core::BlendMode,
        cmds: &mut Vec<Cmd>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        let affine = combine_mesh_affine(current_transform, transform);
        let Some((voff, vcnt, ioff, icnt)) = self.upload_mesh_geometry(mesh, encoder) else {
            return;
        };
        let Some(uoff) = self.alloc_mesh_uniform(mesh_uniform_from_paint(affine, paint)) else {
            return;
        };
        cmds.push(Cmd::VectorMesh {
            voff,
            vcnt,
            ioff,
            icnt,
            uoff,
            blend,
        });
    }

    fn touch_callback_scope(&mut self, scope: CallbackScopeKey) {
        self.callback_scope_clock = self.callback_scope_clock.wrapping_add(1);
        let tick = self.callback_scope_clock;
        self.callback_scope_uses.insert(
            scope,
            CallbackScopeUse {
                tick,
                frame: self.frame_index,
            },
        );
    }

    fn remove_callback_scope(&mut self, scope: &CallbackScopeKey) {
        self.callback_scoped_resources.remove(scope);
        self.callback_scope_uses.remove(scope);
        self.callback_scope_payloads.remove(scope);
    }

    fn evict_callback_scope(&mut self, protected: &HashSet<CallbackScopeKey>) -> bool {
        let candidate = self
            .callback_scoped_resources
            .keys()
            .filter(|scope| !protected.contains(*scope))
            .min_by_key(|scope| {
                self.callback_scope_uses
                    .get(*scope)
                    .map(|usage| usage.tick)
                    .unwrap_or(0)
            })
            .copied();
        let Some(scope) = candidate else {
            return false;
        };
        self.remove_callback_scope(&scope);
        true
    }

    fn prune_callback_scopes(&mut self) {
        let current_frame = self.frame_index;
        let mut dead: Vec<(CallbackScopeKey, u64)> = self
            .callback_scope_uses
            .iter()
            .filter_map(|(scope, usage)| {
                let payload_dead = self
                    .callback_scope_payloads
                    .get(scope)
                    .is_none_or(|weak| weak.upgrade().is_none());
                (!payload_dead).then_some((*scope, usage.tick))
            })
            .collect();
        dead.sort_by_key(|(_, tick)| *tick);
        for (scope, _) in dead {
            self.remove_callback_scope(&scope);
        }

        if self.callback_scoped_resources.len() <= MAX_CALLBACK_SCOPES {
            return;
        }
        let mut inactive: Vec<(CallbackScopeKey, u64)> = self
            .callback_scope_uses
            .iter()
            .filter_map(|(scope, usage)| {
                (usage.frame != current_frame).then_some((*scope, usage.tick))
            })
            .collect();
        inactive.sort_by_key(|(_, tick)| *tick);
        for (scope, _) in inactive {
            if self.callback_scoped_resources.len() <= MAX_CALLBACK_SCOPES {
                break;
            }
            self.remove_callback_scope(&scope);
        }
    }

    pub fn begin_frame(&mut self) -> anyhow::Result<()> {
        if self.frame_active {
            self.last_render_error = Some("renderer frame is already active".into());
            anyhow::bail!("renderer frame is already active");
        }
        self.last_render_error = None;
        self.frame_active = true;
        self.frame_index = self.frame_index.wrapping_add(1);
        self.slug_cache.next_frame();
        if let Some(composite) = self.callback_resources.get_mut::<DepthComposite>() {
            composite.begin_frame();
        }
        for resources in self.callback_scoped_resources.values_mut() {
            if let Some(composite) = resources.get_mut::<DepthComposite>() {
                composite.begin_frame();
            }
        }
        Ok(())
    }

    pub fn end_frame(&mut self) {
        if !self.frame_active {
            return;
        }
        if let Some(composite) = self.callback_resources.get_mut::<DepthComposite>() {
            composite.end_frame();
        }
        for resources in self.callback_scoped_resources.values_mut() {
            if let Some(composite) = resources.get_mut::<DepthComposite>() {
                composite.end_frame();
            }
        }
        self.prune_callback_scopes();
        self.evict_unused_images();
        self.frame_active = false;
    }

    pub fn last_render_error(&self) -> Option<&str> {
        self.last_render_error.as_deref()
    }

    pub fn render_scene_to_encoder(
        &mut self,
        scene: &Scene,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        clear_color_override: Option<[f64; 4]>,
    ) {
        self.render_scene_to_encoder_with_texture(
            scene,
            encoder,
            target_view,
            None,
            clear_color_override,
        )
    }

    /// Same as [`render_scene_to_encoder`](Self::render_scene_to_encoder),
    /// plus the render target texture for isolated-blend backdrop snapshots.
    /// Pass `None` when the texture is unavailable (swapchain views); then
    /// surface-targeted isolated blends are skipped (the isolated source
    /// layer composites nowhere, so the mesh disappears).
    pub fn render_scene_to_encoder_with_texture(
        &mut self,
        scene: &Scene,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        target_texture: Option<&wgpu::Texture>,
        clear_color_override: Option<[f64; 4]>,
    ) {
        /// AABB of a rect under the *plain affine* part of a transform
        /// (linear + translation, no origin re-pivot).
        fn affine_aabb(transform: &Transform, rect: &repose_core::Rect) -> repose_core::Rect {
            let m = transform.linear();
            let (tx, ty) = (transform.translate_x, transform.translate_y);
            let corners = [
                (rect.x, rect.y),
                (rect.x + rect.w, rect.y),
                (rect.x, rect.y + rect.h),
                (rect.x + rect.w, rect.y + rect.h),
            ];
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for (x, y) in corners {
                let wx = m[0] * x + m[1] * y + tx;
                let wy = m[2] * x + m[3] * y + ty;
                min_x = min_x.min(wx);
                min_y = min_y.min(wy);
                max_x = max_x.max(wx);
                max_y = max_y.max(wy);
            }
            repose_core::Rect {
                x: min_x,
                y: min_y,
                w: (max_x - min_x).max(0.0),
                h: (max_y - min_y).max(0.0),
            }
        }

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

        /// Convert a local-space rect + transform to NDC center-based position+size
        /// plus the forward rotation/shear 2x2 (row-major `[m00, m01, m10, m11]`,
        /// scale-free: scale rides in the NDC size). Shaders apply it to quad
        /// corners and its adjugate/determinant inverse to sample positions.
        fn rect_to_instance_ndc(
            rect: repose_core::Rect,
            transform: &Transform,
            fb_w: f32,
            fb_h: f32,
        ) -> ([f32; 4], [f32; 4]) {
            let cx = rect.x + rect.w * 0.5;
            let cy = rect.y + rect.h * 0.5;

            let m = transform.linear();
            let tx = m[0] * cx + m[1] * cy + transform.translate_x;
            let ty = m[2] * cx + m[3] * cy + transform.translate_y;

            let ndc_cx = (tx / fb_w) * 2.0 - 1.0;
            let ndc_cy = 1.0 - (ty / fb_h) * 2.0;
            // NDC size (after scale only, no rotation - rotation is done in shader)
            let ndc_w = (rect.w * transform.scale_x / fb_w) * 2.0;
            let ndc_h = (rect.h * transform.scale_y / fb_h) * 2.0;

            ([ndc_cx, ndc_cy, ndc_w, ndc_h], forward_rs_mat(transform))
        }

        /// Forward rotation+shear 2x2 (row-major, scale-free) for instance
        /// attributes. Identity for untransformed content; degenerate shear
        /// (only from absurd inputs) falls back to identity.
        fn forward_rs_mat(transform: &Transform) -> [f32; 4] {
            let c = transform.rotate.cos();
            let s = transform.rotate.sin();
            let (hx, hy) = (transform.shear_x, transform.shear_y);
            let m = [c - s * hy, c * hx - s, s + c * hy, s * hx + c];
            if (m[0] * m[3] - m[1] * m[2]).abs() < 1e-6 {
                return [1.0, 0.0, 0.0, 1.0];
            }
            m
        }

        fn to_scissor(r: &repose_core::Rect, fb_w: u32, fb_h: u32) -> (u32, u32, u32, u32) {
            if fb_w == 0
                || fb_h == 0
                || !r.x.is_finite()
                || !r.y.is_finite()
                || !r.w.is_finite()
                || !r.h.is_finite()
                || r.w <= 0.0
                || r.h <= 0.0
            {
                return (0, 0, 0, 0);
            }
            let x0 = r.x.floor().max(0.0).min(fb_w as f32);
            let y0 = r.y.floor().max(0.0).min(fb_h as f32);
            let x1 = (r.x + r.w).ceil().max(x0).min(fb_w as f32);
            let y1 = (r.y + r.h).ceil().max(y0).min(fb_h as f32);
            if x1 <= x0 || y1 <= y0 {
                return (0, 0, 0, 0);
            }
            (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
        }

        let fb_w = self.output_width as f32;
        let fb_h = self.output_height as f32;

        let mut passes: Vec<Pass> = Vec::with_capacity(1);
        let clear_color = clear_color_override
            .map(|color| {
                let alpha = color[3];
                [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha]
            })
            .unwrap_or_else(|| {
                let color = scene.clear_color.to_linear();
                let alpha = color[3] as f64;
                [
                    color[0] as f64 * alpha,
                    color[1] as f64 * alpha,
                    color[2] as f64 * alpha,
                    alpha,
                ]
            });
        let mut next_pass_id = 0u64;
        let mut current_pass: Pass = Pass {
            id: next_pass_id,
            target: PassTarget::Surface,
            initial_scissor: (0, 0, self.output_width, self.output_height),
            active_scissor: Some((0, 0, self.output_width, self.output_height)),
            clear_color: Some([
                clear_color[0] as f32,
                clear_color[1] as f32,
                clear_color[2] as f32,
                clear_color[3] as f32,
            ]),
            cmds: Vec::with_capacity(scene.nodes.len()),
        };
        next_pass_id += 1;
        let mut target_stack: Vec<PassTarget> = Vec::new();
        let mut layer_stack: Vec<LayerState> = Vec::new();
        let mut current_target_size: (f32, f32) = (fb_w, fb_h);

        struct Batch {
            rects: Vec<RectInstance>,
            borders: Vec<BorderInstance>,
            ellipses: Vec<EllipseInstance>,
            e_borders: Vec<EllipseBorderInstance>,
            arcs: Vec<ArcInstance>,
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
                    arcs: vec![],
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
                    && self.arcs.is_empty()
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
                    &mut InstancedPipe<ArcInstance>,
                ),
                glyph_pipes: (
                    &mut InstancedPipe<GlyphInstance>,
                    &mut InstancedPipe<GlyphInstance>,
                ),
                nv12_pipe: &mut InstancedPipe<Nv12Instance>,
                device: &wgpu::Device,
                queue: &wgpu::Queue,
                encoder: &mut wgpu::CommandEncoder,
                cmds: &mut Vec<Cmd>,
            ) {
                let (rects, borders, ellipses, e_borders, arcs) = pipes;
                let (masks, colors) = glyph_pipes;

                macro_rules! flush_one {
                    ($buf:ident, $pipe:expr, $variant:ident) => {
                        if !self.$buf.is_empty() {
                            if let Some((off, cnt)) =
                                $pipe.upload(device, queue, encoder, &self.$buf)
                            {
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
                flush_one!(arcs, arcs, Arc);
                flush_one!(masks, masks, GlyphsMask);
                flush_one!(colors, colors, GlyphsColor);

                if !self.nv12s.is_empty() {
                    if let Some((off, cnt)) = nv12_pipe.upload(device, queue, encoder, &self.nv12s)
                    {
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
        self.arcs.reset();
        self.glyph_mask.reset();
        self.glyph_color.reset();
        self.clip_ring.reset();
        self.blur_ring.reset();
        self.nv12.reset();

        self.slug_ring.reset();
        self.mesh_verts.reset();
        self.mesh_indices.reset();
        self.mesh_uniform_head = 0;
        self.projective_ring.reset();
        self.blend_ring.reset();
        // Translator-owned flatten layers are single-frame by construction:
        // drop last frame's textures before translating (their composites
        // were submitted last frame, so GPU-side refs are independent).
        for id in self
            .flatten_layer_ids
            .drain(..)
            .chain(self.producer_layer_ids.drain(..))
        {
            if let Some(layer) = self.layer_pool.remove(&id) {
                self.layer_bytes_total = self.layer_bytes_total.saturating_sub(layer.bytes);
            }
        }
        for snapshot in self.blend_snapshots.values() {
            self.blend_snapshot_bytes_total = self
                .blend_snapshot_bytes_total
                .saturating_sub(snapshot.bytes);
        }
        self.blend_snapshots.clear();
        self.blend_copies.clear();
        let mut batch = Batch::new();
        let mut slug_verts_local: Vec<slug::TessVertex> = Vec::new();
        let mut transform_stack: Vec<Transform> = vec![Transform::identity()];
        let mut flatten_stack: Vec<FlattenRecord> = Vec::new();
        let mut flatten_id_head: u32 = FLATTEN_ID_BASE;
        let mut flatten_ids_used: Vec<u32> = Vec::new();
        let mut scissor_stack: Vec<repose_core::Rect> = Vec::with_capacity(8);
        let mut active_clips: Vec<ActiveClip> = Vec::with_capacity(8);
        let mut root_clip_rect = repose_core::Rect {
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
                            &mut self.arcs,
                        ),
                        (&mut self.glyph_mask, &mut self.glyph_color),
                        &mut self.nv12,
                        &self.device,
                        &self.queue,
                        encoder,
                        &mut current_pass.cmds,
                    )
                }
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
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let (brush_type, grad_kind, color0, color1, grad_p0, grad_p1, tile_mode) =
                        brush_to_shape_fields(brush, rect, current_transform);
                    batch.rects.push(RectInstance {
                        xywh: ndc,
                        radii: radius.map(|r| r.0),
                        brush_type,
                        grad_kind,
                        _pad: [0.0; 2],
                        color0,
                        color1,
                        grad_p0,
                        grad_p1,
                        tile_mode,
                        _pad2: [0.0; 3],
                        fwd_mat,
                    });
                }
                SceneNode::Border {
                    rect,
                    brush,
                    width,
                    radius,
                } => {
                    flush_if_prim_changed!("border", &self.borders);
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let (brush_type, grad_kind, color0, color1, grad_p0, grad_p1, tile_mode) =
                        brush_to_shape_fields(brush, rect, current_transform);
                    batch.borders.push(BorderInstance {
                        xywh: ndc,
                        radii: radius.map(|r| r.0),
                        stroke: width.0,
                        brush_type,
                        _pad: [0.0; 2],
                        grad_kind,
                        color0,
                        color1,
                        grad_p0,
                        grad_p1,
                        tile_mode,
                        _pad2: [0.0; 3],
                        fwd_mat,
                    });
                }
                SceneNode::Ellipse { rect, brush } => {
                    flush_if_prim_changed!("ellipse", &self.ellipses);
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let (brush_type, grad_kind, color0, color1, grad_p0, grad_p1, tile_mode) =
                        brush_to_shape_fields(brush, rect, current_transform);
                    batch.ellipses.push(EllipseInstance {
                        xywh: ndc,
                        brush_type,
                        grad_kind,
                        _pad: [0.0; 2],
                        color0,
                        color1,
                        grad_p0,
                        grad_p1,
                        tile_mode,
                        _pad2: [0.0; 3],
                        fwd_mat,
                    });
                }
                SceneNode::EllipseBorder { rect, brush, width } => {
                    flush_if_prim_changed!("ellipse_border", &self.ellipse_borders);
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let pad_px = width.0 * 0.5 + 2.0;
                    let pad = (pad_px / current_target_size.0.max(1.0)) * 2.0;
                    let (brush_type, grad_kind, color0, color1, grad_p0, grad_p1, tile_mode) =
                        brush_to_shape_fields(brush, rect, current_transform);
                    batch.e_borders.push(EllipseBorderInstance {
                        xywh: ndc,
                        stroke: width.0,
                        pad,
                        brush_type,
                        grad_kind,
                        color0,
                        color1,
                        grad_p0,
                        grad_p1,
                        tile_mode,
                        _pad2: [0.0; 3],
                        fwd_mat,
                    });
                }
                SceneNode::Arc {
                    rect,
                    start_angle,
                    sweep_angle,
                    stroke_width,
                    brush,
                    cap,
                } => {
                    if !start_angle.is_finite()
                        || !sweep_angle.is_finite()
                        || !stroke_width.0.is_finite()
                        || !rect.x.is_finite()
                        || !rect.y.is_finite()
                        || !rect.w.is_finite()
                        || !rect.h.is_finite()
                        || rect.w <= 0.0
                        || rect.h <= 0.0
                        || stroke_width.0 <= 0.0
                        || sweep_angle.abs() <= 1e-6
                    {
                        continue;
                    }
                    flush_if_prim_changed!("arc", &self.arcs);
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0.max(1.0),
                        current_target_size.1.max(1.0),
                    );
                    let pad_px = stroke_width.0 * 0.5 + 2.0;
                    let pad = (pad_px / current_target_size.0.max(1.0)) * 2.0;
                    let cap_val = match cap {
                        StrokeCap::Butt => 0.0,
                        StrokeCap::Round => 1.0,
                        StrokeCap::Square => 2.0,
                    };
                    let (brush_type, grad_kind, color0, color1, grad_p0, grad_p1, tile_mode) =
                        brush_to_shape_fields(brush, rect, current_transform);
                    let (start, sweep) = if *sweep_angle < 0.0 {
                        (*start_angle + *sweep_angle, -*sweep_angle)
                    } else {
                        (*start_angle, *sweep_angle)
                    };
                    batch.arcs.push(ArcInstance {
                        xywh: ndc,
                        start_angle: start,
                        sweep_angle: sweep,
                        stroke: stroke_width.0,
                        pad,
                        brush_type,
                        grad_kind,
                        _pad0: [0.0; 2],
                        color0,
                        color1,
                        grad_p0,
                        grad_p1,
                        tile_mode,
                        cap: cap_val,
                        _pad1: [0.0; 2],
                        fwd_mat,
                    });
                }
                SceneNode::Text {
                    rect,
                    text,
                    color,
                    size,
                    font_family,
                    text_align: _,
                    font_weight,
                    font_style,
                    text_decoration,
                    letter_spacing,
                    line_height: _,
                    extra_style,
                    url: _,
                    font_variation_settings,
                } => {
                    flush_batch!(); // flush any prior primitives

                    let px = size.0;
                    let lh_ratio = rect.h / px;
                    let fw = font_weight.0;
                    let fs = if *font_style == FontStyle::Italic {
                        1
                    } else {
                        0
                    };
                    let shaped = repose_text::shape_line(
                        text.as_ref(),
                        px,
                        lh_ratio,
                        *font_family,
                        fw,
                        fs,
                        letter_spacing.0,
                        font_variation_settings.as_deref(),
                    );
                    let baseline_y = shaped.first().map(|g| rect.y + g.y);

                    let fwd = forward_rs_mat(current_transform);
                    let has_linear = fwd != [1.0, 0.0, 0.0, 1.0];

                    let lin = current_transform.linear();
                    let tr_x = current_transform.translate_x;
                    let tr_y = current_transform.translate_y;

                    let make_glyph_instance =
                        |gx: f32, gy: f32, gw: f32, gh: f32| -> ([f32; 4], [f32; 4]) {
                            if has_linear {
                                let gc_x = gx + gw * 0.5;
                                let gc_y = gy + gh * 0.5;
                                let wc_x = lin[0] * gc_x + lin[1] * gc_y + tr_x;
                                let wc_y = lin[2] * gc_x + lin[3] * gc_y + tr_y;
                                let ww = gw * current_transform.scale_x;
                                let wh = gh * current_transform.scale_y;
                                let ex = fwd[0].abs() * ww * 0.5 + fwd[1].abs() * wh * 0.5;
                                let ey = fwd[2].abs() * ww * 0.5 + fwd[3].abs() * wh * 0.5;
                                let ndc_tl = to_ndc(
                                    wc_x - ex,
                                    wc_y - ey,
                                    ex * 2.0,
                                    ey * 2.0,
                                    current_target_size.0,
                                    current_target_size.1,
                                );
                                let ndc = [
                                    ndc_tl[0] + ndc_tl[2] * 0.5,
                                    ndc_tl[1] + ndc_tl[3] * 0.5,
                                    ndc_tl[2],
                                    ndc_tl[3],
                                ];
                                (ndc, fwd)
                            } else {
                                let (sx, sy) = if current_transform.scale_x == 1.0
                                    && current_transform.scale_y == 1.0
                                {
                                    (gx.round(), gy.round())
                                } else {
                                    (gx, gy)
                                };
                                rect_to_instance_ndc(
                                    repose_core::Rect {
                                        x: sx,
                                        y: sy,
                                        w: gw,
                                        h: gh,
                                    },
                                    current_transform,
                                    current_target_size.0,
                                    current_target_size.1,
                                )
                            }
                        };

                    let baseline_shift_y: f32 = px * extra_style.baseline_shift.0;

                    let (
                        draws_fill,
                        is_stroke,
                        stroke_width,
                        stroke_cap,
                        stroke_join,
                        stroke_miter,
                        stroke_path_effect,
                    ) = match &extra_style.draw_style {
                        repose_core::DrawStyle::Stroke {
                            width,
                            cap,
                            join,
                            miter,
                            path_effect,
                        } => (
                            false,
                            true,
                            *width,
                            *cap,
                            *join,
                            *miter,
                            path_effect.clone(),
                        ),
                        repose_core::DrawStyle::FillAndStroke {
                            width,
                            cap,
                            join,
                            miter,
                            path_effect,
                        } => (true, true, *width, *cap, *join, *miter, path_effect.clone()),
                        _ => (
                            true,
                            false,
                            0.0,
                            repose_core::StrokeCap::Butt,
                            repose_core::StrokeJoin::Miter,
                            4.0,
                            None,
                        ),
                    };
                    let stroke_tess_key = if is_stroke {
                        Some(slug::StrokeTessKey::new(
                            stroke_width,
                            stroke_cap,
                            stroke_join,
                            stroke_miter,
                            &stroke_path_effect,
                        ))
                    } else {
                        None
                    };

                    for sg in shaped {
                        let gx = rect.x + sg.x + sg.bearing_x;
                        let gy = rect.y + sg.y - sg.bearing_y + baseline_shift_y;

                        // Vector glyph path: tessellated geometry with MSAA.
                        if self.slug_enabled {
                            let ck = repose_text::lookup_cache_key(sg.key, sg.px);
                            if let Some(ref ck) = ck {
                                // Check if cached.
                                let need_tessellate = self.slug_cache.get(ck).is_none_or(|g| {
                                    (draws_fill && g.fill_vertices.is_none())
                                        || (is_stroke
                                            && !g
                                                .stroke_variants
                                                .contains_key(stroke_tess_key.as_ref().unwrap()))
                                });
                                if need_tessellate {
                                    if let Some((ck2, commands)) =
                                        repose_text::lookup_and_extract_outline(sg.key, sg.px)
                                    {
                                        let font_size_px = f32::from_bits(ck2.font_size_bits);
                                        if draws_fill {
                                            self.slug_cache.get_or_insert(
                                                ck2,
                                                font_size_px,
                                                &commands,
                                            );
                                        }
                                        if is_stroke {
                                            self.slug_cache.get_or_insert_stroke(
                                                ck2,
                                                font_size_px,
                                                &commands,
                                                stroke_width,
                                                stroke_cap,
                                                stroke_join,
                                                stroke_miter,
                                                &stroke_path_effect,
                                            );
                                        }
                                    }
                                } else {
                                    self.slug_cache.touch(ck);
                                }
                            }
                            if let Some(entry) = ck.as_ref().and_then(|ck| self.slug_cache.get(ck))
                            {
                                let ox = rect.x + sg.x;
                                let oy = rect.y + sg.y + baseline_shift_y;
                                let scx = current_transform.scale_x;
                                let scy = current_transform.scale_y;
                                let ttx = current_transform.translate_x;
                                let tty = current_transform.translate_y;

                                let tf = |x: f32, y: f32| -> (f32, f32) {
                                    if has_linear {
                                        (
                                            lin[0] * x + lin[1] * y + ttx,
                                            lin[2] * x + lin[3] * y + tty,
                                        )
                                    } else {
                                        (x * scx + ttx, y * scy + tty)
                                    }
                                };

                                let tw = current_target_size.0;
                                let th = current_target_size.1;

                                let mut emit = |verts: &[[f32; 2]]| {
                                    for &v in verts {
                                        let (sx, sy) = tf(ox + v[0] * px, oy - v[1] * px);
                                        let ndc_x = sx / tw * 2.0 - 1.0;
                                        let ndc_y = -(sy / th) * 2.0 + 1.0;
                                        slug_verts_local.push(slug::TessVertex {
                                            ndc_pos: [ndc_x, ndc_y],
                                            color: color.to_linear(),
                                        });
                                    }
                                };
                                if draws_fill {
                                    emit(entry.fill_vertices.as_deref().unwrap_or(&[]));
                                }
                                if is_stroke {
                                    let key = stroke_tess_key.as_ref().unwrap();
                                    emit(
                                        entry
                                            .stroke_variants
                                            .get(key)
                                            .map(|v| v.as_slice())
                                            .unwrap_or(&[]),
                                    );
                                }

                                if !draws_fill {
                                    // Stroke glyphs cannot use atlas fallback...
                                    continue;
                                }
                                continue;
                            }
                        }

                        if !draws_fill {
                            // Don't use atlas fallback for strokes too
                            continue;
                        }

                        if let Some(info) = self.upload_glyph_color(sg.key, sg.px) {
                            let (ndc, fwd_mat) = make_glyph_instance(gx, gy, info.w, info.h);
                            batch.colors.push(GlyphInstance {
                                xywh: ndc,
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                                fwd_mat,
                            });
                        } else if let Some(info) = self.upload_glyph_mask(sg.key, sg.px) {
                            let (ndc, fwd_mat) = make_glyph_instance(gx, gy, info.w, info.h);
                            batch.masks.push(GlyphInstance {
                                xywh: ndc,
                                uv: [info.u0, info.v1, info.u1, info.v0],
                                color: color.to_linear(),
                                fwd_mat,
                            });
                        }
                    }

                    // Upload slug vertices if any
                    if !slug_verts_local.is_empty() {
                        let bytes = bytemuck::cast_slice(&slug_verts_local);
                        let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                        let Ok(count) = u32::try_from(slug_verts_local.len()) else {
                            slug_verts_local.clear();
                            continue;
                        };
                        if self
                            .slug_ring
                            .grow_to_fit(&self.device, encoder, byte_len)
                            .is_ok()
                            && let Ok(off) = self.slug_ring.alloc_write(&self.queue, bytes)
                        {
                            current_pass
                                .cmds
                                .push(Cmd::GlyphsVector { off, cnt: count });
                        }
                        slug_verts_local.clear();
                    }

                    // Text decoration: underline / strikethrough
                    if (text_decoration.underline || text_decoration.strikethrough)
                        && let Some(baseline_y) = baseline_y
                    {
                        flush_batch!();
                        current_prim = Some("rect");
                        let deco_color = text_decoration.color.unwrap_or(*color);
                        let thickness = (px * 0.07).max(1.0);

                        if text_decoration.underline {
                            let dy = baseline_y + px * 0.1;
                            let (ndc, fwd_mat) = rect_to_instance_ndc(
                                repose_core::Rect {
                                    x: rect.x,
                                    y: dy,
                                    w: rect.w,
                                    h: thickness,
                                },
                                current_transform,
                                current_target_size.0,
                                current_target_size.1,
                            );
                            batch.rects.push(RectInstance {
                                xywh: ndc,
                                radii: [0.0; 4],
                                brush_type: 0,
                                grad_kind: 0,
                                _pad: [0.0; 2],
                                color0: deco_color.to_linear(),
                                color1: [0.0; 4],
                                grad_p0: [0.0; 2],
                                grad_p1: [0.0; 2],
                                tile_mode: 0,
                                _pad2: [0.0; 3],
                                fwd_mat,
                            });
                        }
                        if text_decoration.strikethrough {
                            let sy = baseline_y - px * 0.3;
                            let (ndc, fwd_mat) = rect_to_instance_ndc(
                                repose_core::Rect {
                                    x: rect.x,
                                    y: sy,
                                    w: rect.w,
                                    h: thickness,
                                },
                                current_transform,
                                current_target_size.0,
                                current_target_size.1,
                            );
                            batch.rects.push(RectInstance {
                                xywh: ndc,
                                radii: [0.0; 4],
                                brush_type: 0,
                                grad_kind: 0,
                                _pad: [0.0; 2],
                                color0: deco_color.to_linear(),
                                color1: [0.0; 4],
                                grad_p0: [0.0; 2],
                                grad_p1: [0.0; 2],
                                tile_mode: 0,
                                _pad2: [0.0; 3],
                                fwd_mat,
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
                    flush_batch!();

                    // Update usage timestamp for eviction, lazily re-uploading
                    // evicted RGBA images from their retained source.
                    let (img_w, img_h, is_nv12) = match self.resolve_image_for_draw(*handle) {
                        Some(wh) => wh,
                        None => {
                            log::warn!("Image handle {} not found", handle);
                            continue;
                        }
                    };

                    let src_w = img_w as f32;
                    let src_h = img_h as f32;

                    let dst_w = rect.w.max(0.0);
                    let dst_h = rect.h.max(0.0);
                    if dst_w <= 0.0 || dst_h <= 0.0 {
                        continue;
                    }

                    let (draw_rect, uv_rect) = match fit {
                        repose_core::view::ImageFit::Contain => {
                            let scale = (dst_w / src_w).min(dst_h / src_h);
                            let w = src_w * scale;
                            let h = src_h * scale;
                            (
                                repose_core::Rect {
                                    x: rect.x + (dst_w - w) * 0.5,
                                    y: rect.y + (dst_h - h) * 0.5,
                                    w,
                                    h,
                                },
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
                            (*rect, [u0, 1.0 - v0, u1, 1.0 - v1])
                        }
                        repose_core::view::ImageFit::FitWidth => {
                            let scale = dst_w / src_w;
                            (
                                repose_core::Rect {
                                    x: rect.x,
                                    y: rect.y + (dst_h - src_h * scale) * 0.5,
                                    w: dst_w,
                                    h: src_h * scale,
                                },
                                [0.0, 1.0, 1.0, 0.0],
                            )
                        }
                        repose_core::view::ImageFit::FitHeight => {
                            let scale = dst_h / src_h;
                            (
                                repose_core::Rect {
                                    x: rect.x + (dst_w - src_w * scale) * 0.5,
                                    y: rect.y,
                                    w: src_w * scale,
                                    h: dst_h,
                                },
                                [0.0, 1.0, 1.0, 0.0],
                            )
                        }
                        repose_core::view::ImageFit::FillBounds => (*rect, [0.0, 1.0, 1.0, 0.0]),
                        repose_core::view::ImageFit::Inside => {
                            let scale = (dst_w / src_w).min(dst_h / src_h).min(1.0);
                            let w = src_w * scale;
                            let h = src_h * scale;
                            (
                                repose_core::Rect {
                                    x: rect.x + (dst_w - w) * 0.5,
                                    y: rect.y + (dst_h - h) * 0.5,
                                    w,
                                    h,
                                },
                                [0.0, 1.0, 1.0, 0.0],
                            )
                        }
                        repose_core::view::ImageFit::None => {
                            (
                                repose_core::Rect {
                                    x: rect.x,
                                    y: rect.y,
                                    w: src_w.min(dst_w),
                                    h: src_h.min(dst_h),
                                },
                                // If larger than dst, crop top-left of source:
                                [
                                    0.0,
                                    1.0,
                                    (dst_w / src_w).min(1.0),
                                    1.0 - (dst_h / src_h).min(1.0),
                                ],
                            )
                        }
                        _ => continue,
                    };

                    let (ndc_center, fwd_mat) = rect_to_instance_ndc(
                        draw_rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );

                    if is_nv12 {
                        let (uv_x_offset, uv_y_offset) =
                            if let Some(ImageTex::Nv12 {
                                w, h, color_info, ..
                            }) = self.images.get(handle)
                            {
                                let chroma_w = w.div_ceil(2).max(1) as f32;
                                let chroma_h = h.div_ceil(2).max(1) as f32;
                                match color_info.chroma_siting {
                                    ChromaSiting::Center => (0.0, 0.0),
                                    ChromaSiting::Left => (-0.5 / chroma_w, 0.0),
                                    ChromaSiting::TopLeft => (-0.5 / chroma_w, -0.5 / chroma_h),
                                }
                            } else {
                                (0.0, 0.0)
                            };

                        let inst = Nv12Instance {
                            xywh: ndc_center,
                            uv: uv_rect,
                            color: tint.to_linear(),
                            uv_x_offset,
                            uv_y_offset,
                            fwd_mat,
                        };
                        if let Some((off, _)) =
                            self.nv12
                                .upload(&self.device, &self.queue, encoder, &[inst])
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
                            xywh: ndc_center,
                            uv: uv_rect,
                            color: tint.to_linear(),
                            fwd_mat,
                        };
                        if let Some((off, _)) =
                            self.glyph_color
                                .upload(&self.device, &self.queue, encoder, &[inst])
                        {
                            current_pass.cmds.push(Cmd::ImageRgba {
                                off,
                                cnt: 1,
                                handle: *handle,
                            });
                        }
                    }
                }
                SceneNode::Coverage {
                    rect,
                    handle,
                    color,
                } => {
                    flush_batch!();
                    // Unknown handles are skipped (same policy as images);
                    // the lookup also marks the tile used for eviction.
                    let Some((tile_w, tile_h)) = self.coverage_dimensions(*handle) else {
                        log::warn!("Coverage handle {handle} not found");
                        continue;
                    };
                    // The tile composites at its registered size; `rect`
                    // positions its top-left.
                    let draw_rect = repose_core::Rect {
                        x: rect.x,
                        y: rect.y,
                        w: tile_w as f32,
                        h: tile_h as f32,
                    };
                    let (ndc_center, fwd_mat) = rect_to_instance_ndc(
                        draw_rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let inst = GlyphInstance {
                        xywh: ndc_center,
                        uv: [0.0, 1.0, 1.0, 0.0],
                        color: color.to_linear(),
                        fwd_mat,
                    };
                    if let Some((off, _)) =
                        self.glyph_color
                            .upload(&self.device, &self.queue, encoder, &[inst])
                    {
                        current_pass.cmds.push(Cmd::Coverage {
                            off,
                            cnt: 1,
                            handle: *handle,
                        });
                    }
                }
                SceneNode::PushClip { rect, radius, op } => {
                    flush_batch!();

                    let is_diff = matches!(op, repose_core::ClipOp::Difference);
                    let t_identity = Transform::identity();
                    let current_transform = transform_stack.last().unwrap_or(&t_identity);
                    let transformed = affine_aabb(current_transform, rect);
                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let next_scissor = if is_diff {
                        top
                    } else {
                        intersect(top, transformed)
                    };
                    scissor_stack.push(next_scissor);
                    let scissor = to_scissor(
                        &next_scissor,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    let has_inverse = active_clips.iter().any(ActiveClip::difference);
                    let mut blocked =
                        current_pass.active_scissor.is_none() || (has_inverse && !is_diff);
                    if has_inverse && is_diff {
                        log::error!(
                            "unsupported clip nesting: Difference cannot contain a Difference clip"
                        );
                        blocked = true;
                    }
                    let has_area = transformed.x.is_finite()
                        && transformed.y.is_finite()
                        && transformed.w.is_finite()
                        && transformed.h.is_finite()
                        && transformed.w > 0.0
                        && transformed.h > 0.0;
                    let mut applied = false;
                    let mut off = 0;
                    if !blocked && has_area && (is_diff || scissor.2 > 0 && scissor.3 > 0) {
                        let clip_ndc_tl = to_ndc(
                            transformed.x,
                            transformed.y,
                            transformed.w,
                            transformed.h,
                            current_target_size.0,
                            current_target_size.1,
                        );
                        let inst = ClipInstance {
                            xywh: [
                                clip_ndc_tl[0] + clip_ndc_tl[2] * 0.5,
                                clip_ndc_tl[1] + clip_ndc_tl[3] * 0.5,
                                clip_ndc_tl[2],
                                clip_ndc_tl[3],
                            ],
                            radii: radius.map(|r| r.0),
                            fwd_mat: [1.0, 0.0, 0.0, 1.0],
                        };
                        let bytes = bytemuck::bytes_of(&inst);
                        if self
                            .clip_ring
                            .grow_to_fit(
                                &self.device,
                                encoder,
                                u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                            )
                            .is_ok()
                            && let Ok(new_off) = self.clip_ring.alloc_write(&self.queue, bytes)
                        {
                            off = new_off;
                            applied = true;
                            current_pass.cmds.push(Cmd::ClipPush {
                                off,
                                cnt: 1,
                                scissor,
                                difference: is_diff,
                                applied,
                            });
                        } else {
                            blocked = !is_diff;
                        }
                    } else if !is_diff {
                        blocked = true;
                    }
                    current_pass.active_scissor = if blocked {
                        None
                    } else {
                        (scissor.2 > 0 && scissor.3 > 0).then_some(scissor)
                    };
                    active_clips.push(ActiveClip::Rect {
                        off,
                        cnt: u32::from(applied),
                        rect: transformed,
                        radii: radius.map(|r| r.0),
                        difference: is_diff,
                        applied,
                        blocked,
                    });
                }
                SceneNode::PopClip => {
                    flush_batch!();

                    if scissor_stack.is_empty() || active_clips.is_empty() {
                        log::error!("PopClip does not match an active clip");
                    } else {
                        scissor_stack.pop();
                    }
                    let clip = active_clips.pop();
                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let scissor = to_scissor(
                        &top,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    current_pass.active_scissor =
                        (scissor.2 > 0 && scissor.3 > 0).then_some(scissor);
                    match clip {
                        Some(ActiveClip::Rect {
                            off,
                            cnt,
                            difference,
                            applied,
                            ..
                        }) if applied && cnt > 0 => {
                            current_pass.cmds.push(Cmd::ClipPop {
                                off,
                                cnt,
                                scissor,
                                difference,
                                applied: true,
                            });
                        }
                        Some(ActiveClip::Vector { .. }) => {
                            log::error!("PopClip matched a vector clip");
                        }
                        None => {}
                        _ => {}
                    }
                }
                SceneNode::Shadow {
                    rect,
                    radius,
                    elevation: _,
                    color,
                } => {
                    flush_if_prim_changed!("rect", &self.rects);
                    let (ndc, fwd_mat) = rect_to_instance_ndc(
                        *rect,
                        current_transform,
                        current_target_size.0,
                        current_target_size.1,
                    );
                    let (brush_type, color0, _color1, _grad_p0, _grad_p1) =
                        brush_to_instance_fields(&Brush::Solid(*color));
                    batch.rects.push(RectInstance {
                        xywh: ndc,
                        radii: radius.map(|r| r.0),
                        brush_type,
                        grad_kind: 0,
                        _pad: [0.0; 2],
                        color0,
                        color1: [0.0; 4],
                        grad_p0: [0.0; 2],
                        grad_p1: [0.0; 2],
                        tile_mode: 0,
                        _pad2: [0.0; 3],
                        fwd_mat,
                    });
                }
                SceneNode::PushTransform { transform } => {
                    flush_batch!(); // flush before transform change
                    if transform.has_perspective() {
                        // True perspective cannot ride the affine fast path:
                        // flatten the subtree into an offscreen layer and
                        // composite it back projectively (CSS-style). See
                        // `push_perspective_layer`.
                        let top = *transform_stack.last().unwrap_or(&t_identity);
                        self.push_perspective_layer(
                            *transform,
                            top,
                            &mut transform_stack,
                            &mut scissor_stack,
                            &mut root_clip_rect,
                            &mut current_target_size,
                            &mut current_pass,
                            &mut active_clips,
                            &mut passes,
                            &mut target_stack,
                            &mut flatten_stack,
                            &mut flatten_id_head,
                            &mut next_pass_id,
                            &mut flatten_ids_used,
                            encoder,
                        );
                    } else {
                        let combined = current_transform.combine(transform);
                        transform_stack.push(combined);
                    }
                }
                SceneNode::PopTransform => {
                    flush_batch!(); // flush before transform change
                    if let Some(rec) = flatten_stack.last() {
                        // A flatten level closes when the stack is back to the
                        // two entries this flatten pushed (stripped transform +
                        // layer-local shift); deeper plain pushes close first.
                        if transform_stack.len() == rec.stack_len + 2 {
                            let rec = flatten_stack.pop().expect("checked above");
                            transform_stack.pop();
                            transform_stack.pop();
                            self.pop_perspective_layer(
                                rec,
                                &mut scissor_stack,
                                &mut root_clip_rect,
                                &mut current_target_size,
                                &mut current_pass,
                                &mut active_clips,
                                &mut passes,
                                &mut target_stack,
                                &mut next_pass_id,
                                encoder,
                            );
                            continue;
                        }
                    }
                    transform_stack.pop();
                }
                SceneNode::BeginLayer {
                    rect,
                    layer_id,
                    alpha,
                    blur_radius_x,
                    blur_radius_y,
                    rectangle_edge,
                } => {
                    flush_batch!();
                    let width_f = rect.w.round();
                    let height_f = rect.h.round();
                    let max_dimension = self.device.limits().max_texture_dimension_2d as f32;
                    if !width_f.is_finite()
                        || !height_f.is_finite()
                        || width_f < 1.0
                        || height_f < 1.0
                        || width_f > max_dimension
                        || height_f > max_dimension
                    {
                        log::warn!("BeginLayer dimensions are invalid");
                        continue;
                    }
                    let width = width_f as u32;
                    let height = height_f as u32;
                    let parent_scissors =
                        std::mem::replace(&mut scissor_stack, Vec::with_capacity(8));
                    let parent_root_clip = std::mem::replace(
                        &mut root_clip_rect,
                        repose_core::Rect {
                            x: 0.0,
                            y: 0.0,
                            w: width as f32,
                            h: height as f32,
                        },
                    );
                    scissor_stack.push(root_clip_rect);
                    let layer_pass_id = next_pass_id;
                    next_pass_id += 1;
                    let parent_clips = std::mem::take(&mut active_clips);
                    let mut layer_pass = Pass {
                        id: layer_pass_id,
                        target: PassTarget::Layer(*layer_id),
                        initial_scissor: (0, 0, width, height),
                        active_scissor: Some((0, 0, width, height)),
                        clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                        cmds: Vec::new(),
                    };
                    active_clips = self.replay_active_clips(
                        &parent_clips,
                        (rect.x, rect.y),
                        (width as f32, height as f32),
                        &mut layer_pass,
                        encoder,
                    );
                    let saved = std::mem::replace(&mut current_pass, layer_pass);
                    let parent_transform = *current_transform;
                    if let Some(top) = transform_stack.last_mut() {
                        *top = Transform::identity();
                    }
                    layer_stack.push(LayerState {
                        layer_id: *layer_id,
                        parent_target: saved.target,
                        parent_scissors,
                        parent_root_clip,
                        parent_size: current_target_size,
                        parent_transform,
                        parent_scissor: saved.active_scissor,
                        parent_clips,
                        alpha: *alpha,
                        blur: (blur_radius_x.0, blur_radius_y.0),
                        rectangle_edge: *rectangle_edge,
                    });
                    passes.push(saved);
                    if self.get_or_create_layer(*layer_id, width, height, *rect)
                        && !self.producer_layer_ids.contains(layer_id)
                    {
                        self.producer_layer_ids.push(*layer_id);
                    }
                    current_target_size = (width as f32, height as f32);
                }
                SceneNode::EndLayer { layer_id } => {
                    flush_batch!();
                    if layer_stack.last().map(|state| state.layer_id) != Some(*layer_id) {
                        log::warn!("EndLayer {} does not match active layer", layer_id);
                        continue;
                    }
                    let state = layer_stack.pop().expect("checked above");
                    scissor_stack = state.parent_scissors;
                    root_clip_rect = state.parent_root_clip;
                    active_clips = state.parent_clips;
                    if let Some(top) = transform_stack.last_mut() {
                        *top = state.parent_transform;
                    }
                    current_target_size = state.parent_size;
                    let resumed_pass_id = next_pass_id;
                    next_pass_id += 1;
                    let saved = std::mem::replace(
                        &mut current_pass,
                        Pass {
                            id: resumed_pass_id,
                            target: state.parent_target,
                            initial_scissor: state.parent_scissor.unwrap_or((0, 0, 1, 1)),
                            active_scissor: state.parent_scissor,
                            clear_color: None,
                            cmds: Vec::new(),
                        },
                    );
                    passes.push(saved);
                    let Some(layer) = self.layer_pool.get(layer_id).cloned() else {
                        continue;
                    };
                    let layer_rect = repose_core::Rect {
                        x: layer.rect_px.0,
                        y: layer.rect_px.1,
                        w: layer.rect_px.2,
                        h: layer.rect_px.3,
                    };
                    let local_layer_rect = affine_aabb(&state.parent_transform, &layer_rect);
                    let (parent_width, parent_height) = state.parent_size;
                    if state.blur.0 > 0.0 || state.blur.1 > 0.0 {
                        let blur_x = state.blur.0 * 1.5;
                        let blur_y = state.blur.1 * 1.5;
                        let rect = repose_core::Rect {
                            x: local_layer_rect.x - blur_x,
                            y: local_layer_rect.y - blur_y,
                            w: local_layer_rect.w + blur_x * 2.0,
                            h: local_layer_rect.h + blur_y * 2.0,
                        };
                        let ndc =
                            to_ndc(rect.x, rect.y, rect.w, rect.h, parent_width, parent_height);
                        let inst = BlurInstance {
                            xywh: [ndc[0] + ndc[2] * 0.5, ndc[1] + ndc[3] * 0.5, ndc[2], ndc[3]],
                            uv: [0.0, 0.0, 1.0, 1.0],
                            color: [1.0, 1.0, 1.0, state.alpha],
                            blur_uv: [
                                (state.blur.0 * 1.5) / layer.width.max(1) as f32,
                                (state.blur.1 * 1.5) / layer.height.max(1) as f32,
                            ],
                            fwd_mat: [1.0, 0.0, 0.0, 1.0],
                            edge_mode: if state.rectangle_edge { 0 } else { 1 },
                            _pad: [0.0; 3],
                        };
                        if self
                            .blur_ring
                            .grow_to_fit(
                                &self.device,
                                encoder,
                                std::mem::size_of::<BlurInstance>() as u64,
                            )
                            .is_ok()
                            && let Ok(off) = self
                                .blur_ring
                                .alloc_write(&self.queue, bytemuck::bytes_of(&inst))
                        {
                            current_pass.cmds.push(Cmd::CompositeBlur {
                                off,
                                cnt: 1,
                                layer_id: *layer_id,
                            });
                        }
                    } else {
                        let ndc = to_ndc(
                            local_layer_rect.x,
                            local_layer_rect.y,
                            local_layer_rect.w,
                            local_layer_rect.h,
                            parent_width,
                            parent_height,
                        );
                        let inst = GlyphInstance {
                            xywh: [ndc[0] + ndc[2] * 0.5, ndc[1] + ndc[3] * 0.5, ndc[2], ndc[3]],
                            uv: [0.0, 1.0, 1.0, 0.0],
                            color: [1.0, 1.0, 1.0, state.alpha],
                            fwd_mat: [1.0, 0.0, 0.0, 1.0],
                        };
                        if let Some((off, cnt)) =
                            self.glyph_color
                                .upload(&self.device, &self.queue, encoder, &[inst])
                        {
                            current_pass.cmds.push(Cmd::CompositeLayer {
                                off,
                                cnt,
                                layer_id: *layer_id,
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
                        let layer_rect = repose_core::Rect {
                            x: layer.rect_px.0,
                            y: layer.rect_px.1,
                            w: layer.rect_px.2,
                            h: layer.rect_px.3,
                        };
                        let local_layer_rect =
                            affine_aabb(transform_stack.last().unwrap_or(&t_identity), &layer_rect);
                        let blur_x = blur_px.0.max(0.0) * 1.5;
                        let blur_y = blur_px.0.max(0.0) * 1.5;
                        let sx = local_layer_rect.x + offset_px.0.0 - blur_x;
                        let sy = local_layer_rect.y + offset_px.1.0 - blur_y;
                        let sw = local_layer_rect.w + blur_x * 2.0;
                        let sh = local_layer_rect.h + blur_y * 2.0;
                        let bw_uv = blur_x / layer.width.max(1) as f32;
                        let bh_uv = blur_y / layer.height.max(1) as f32;
                        let ndc_tl =
                            to_ndc(sx, sy, sw, sh, current_target_size.0, current_target_size.1);
                        let inst = BlurInstance {
                            xywh: [
                                ndc_tl[0] + ndc_tl[2] * 0.5,
                                ndc_tl[1] + ndc_tl[3] * 0.5,
                                ndc_tl[2],
                                ndc_tl[3],
                            ],
                            uv: [0.0, 0.0, 1.0, 1.0],
                            color: color.to_linear(),
                            blur_uv: [bw_uv, bh_uv],
                            fwd_mat: [1.0, 0.0, 0.0, 1.0],
                            edge_mode: 0,
                            _pad: [0.0; 3],
                        };
                        if self
                            .blur_ring
                            .grow_to_fit(
                                &self.device,
                                encoder,
                                std::mem::size_of::<BlurInstance>() as u64,
                            )
                            .is_ok()
                            && let Ok(off) = self
                                .blur_ring
                                .alloc_write(&self.queue, bytemuck::bytes_of(&inst))
                        {
                            current_pass.cmds.push(Cmd::CompositeShadow {
                                off,
                                cnt: 1,
                                layer_id: *layer_id,
                            });
                        }
                    }
                }
                SceneNode::VectorMesh {
                    mesh,
                    transform,
                    paint,
                    clip: _,
                    blend,
                } => {
                    flush_batch!();
                    let paint_alpha = match paint {
                        repose_core::PaintDesc::Linear {
                            start_color,
                            end_color,
                            ..
                        }
                        | repose_core::PaintDesc::Radial {
                            start_color,
                            end_color,
                            ..
                        }
                        | repose_core::PaintDesc::Sweep {
                            start_color,
                            end_color,
                            ..
                        } => Some(start_color.3.min(end_color.3) as f32 / 255.0),
                        _ => None,
                    };
                    let translucent_fixed_blend =
                        matches!(
                            blend,
                            repose_core::BlendMode::Add
                                | repose_core::BlendMode::Multiply
                                | repose_core::BlendMode::Screen
                                | repose_core::BlendMode::Darken
                                | repose_core::BlendMode::Lighten
                        ) && (mesh.vertices.iter().any(|vertex| vertex.color[3] < 0.9999)
                            || paint_alpha.is_some_and(|alpha| alpha < 0.9999));
                    if blend.needs_isolation() || translucent_fixed_blend {
                        let blend_scissor = to_scissor(
                            &scissor_stack.last().copied().unwrap_or(root_clip_rect),
                            current_target_size.0 as u32,
                            current_target_size.1 as u32,
                        );
                        self.emit_isolated_blend(
                            mesh.clone(),
                            *transform,
                            *paint,
                            *blend,
                            current_transform,
                            &mut current_pass,
                            &active_clips,
                            &mut passes,
                            &mut flatten_id_head,
                            &mut next_pass_id,
                            &mut flatten_ids_used,
                            blend_scissor,
                            encoder,
                            fb_w,
                            fb_h,
                        );
                    } else {
                        let t_identity = Transform::identity();
                        let current_transform = transform_stack.last().unwrap_or(&t_identity);
                        self.emit_vector_mesh(
                            current_transform,
                            mesh,
                            *transform,
                            paint,
                            *blend,
                            &mut current_pass.cmds,
                            encoder,
                        );
                    }
                }
                SceneNode::VectorOverlay { meshes } => {
                    flush_batch!();
                    for m in meshes.iter() {
                        let Some((voff, vcnt, ioff, icnt)) = self.upload_mesh_geometry(m, encoder)
                        else {
                            continue;
                        };
                        let Some(uoff) = self.alloc_mesh_uniform(MeshUniform::identity()) else {
                            continue;
                        };
                        current_pass.cmds.push(Cmd::VectorOverlay {
                            voff,
                            vcnt,
                            ioff,
                            icnt,
                            uoff,
                        });
                    }
                }
                SceneNode::PushVectorClip { mesh, op } => {
                    flush_batch!();
                    let difference = matches!(op, repose_core::ClipOp::Difference);
                    let t_identity = Transform::identity();
                    let current_transform = transform_stack.last().unwrap_or(&t_identity);
                    let affine =
                        combine_mesh_affine(current_transform, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                    let aabb = mesh_aabb(mesh, affine);
                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let next = if difference {
                        top
                    } else {
                        intersect(top, aabb)
                    };
                    scissor_stack.push(next);
                    let scissor = to_scissor(
                        &next,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    let has_inverse = active_clips.iter().any(ActiveClip::difference);
                    let mut blocked = current_pass.active_scissor.is_none() || has_inverse;
                    if has_inverse {
                        log::error!(
                            "unsupported clip nesting: a Difference clip cannot contain another vector clip"
                        );
                    }
                    let has_area = aabb.x.is_finite()
                        && aabb.y.is_finite()
                        && aabb.w.is_finite()
                        && aabb.h.is_finite()
                        && aabb.w > 0.0
                        && aabb.h > 0.0;
                    let mut applied = false;
                    let mut geometry = (0, 0, 0, 0);
                    let mut uoff = 0;
                    if !blocked && has_area && (difference || scissor.2 > 0 && scissor.3 > 0) {
                        if let Some((voff, vcnt, ioff, icnt)) =
                            self.upload_mesh_geometry(mesh, encoder)
                            && let Some(slot) = self.alloc_mesh_uniform(mesh_uniform_from_paint(
                                affine,
                                &repose_core::PaintDesc::Solid,
                            ))
                        {
                            geometry = (voff, vcnt, ioff, icnt);
                            uoff = slot;
                            applied = true;
                            current_pass.cmds.push(Cmd::VectorClipPush {
                                voff,
                                vcnt,
                                ioff,
                                icnt,
                                uoff,
                                scissor,
                                difference,
                                applied,
                            });
                        } else {
                            blocked = !difference;
                        }
                    } else if !difference {
                        blocked = true;
                    }
                    current_pass.active_scissor = if blocked {
                        None
                    } else {
                        (scissor.2 > 0 && scissor.3 > 0).then_some(scissor)
                    };
                    active_clips.push(ActiveClip::Vector {
                        voff: geometry.0,
                        vcnt: geometry.1,
                        ioff: geometry.2,
                        icnt: geometry.3,
                        uoff,
                        mesh: mesh.clone(),
                        affine,
                        difference,
                        applied,
                        blocked,
                    });
                }
                SceneNode::PopVectorClip => {
                    flush_batch!();
                    if scissor_stack.is_empty() || active_clips.is_empty() {
                        log::error!("PopVectorClip does not match an active clip");
                    } else {
                        scissor_stack.pop();
                    }
                    let clip = active_clips.pop();
                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let scissor = to_scissor(
                        &top,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    current_pass.active_scissor =
                        (scissor.2 > 0 && scissor.3 > 0).then_some(scissor);
                    match clip {
                        Some(ActiveClip::Vector {
                            voff,
                            vcnt,
                            ioff,
                            icnt,
                            uoff,
                            difference,
                            applied,
                            ..
                        }) if applied && vcnt > 0 && icnt > 0 => {
                            current_pass.cmds.push(Cmd::VectorClipPop {
                                voff,
                                vcnt,
                                ioff,
                                icnt,
                                uoff,
                                scissor,
                                difference,
                                applied: true,
                            });
                        }
                        Some(ActiveClip::Rect { .. }) => {
                            log::error!("PopVectorClip matched a rounded-rect clip");
                        }
                        None => {}
                        _ => {}
                    }
                }
                SceneNode::Callback { rect, payload } => {
                    flush_batch!();
                    let t = transform_stack
                        .last()
                        .copied()
                        .unwrap_or(Transform::identity());
                    let transformed = affine_aabb(&t, rect);
                    let top = scissor_stack.last().copied().unwrap_or(root_clip_rect);
                    let clip_rect = intersect(transformed, top);
                    let scissor = to_scissor(
                        &clip_rect,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    let restore_scissor = to_scissor(
                        &top,
                        current_target_size.0 as u32,
                        current_target_size.1 as u32,
                    );
                    current_pass.cmds.push(Cmd::Callback {
                        rect: transformed,
                        clip_rect,
                        scissor,
                        restore_scissor,
                        callback_id: Arc::as_ptr(payload) as *const () as usize,
                        payload: payload.clone(),
                    });
                }
                _ => {}
            }
        }

        flush_batch!();
        passes.push(current_pass);

        {
            let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
            let mut prepare_list: Vec<(usize, Arc<Callback>)> = Vec::new();
            for node in &scene.nodes {
                if let SceneNode::Callback { payload, .. } = node
                    && payload.downcast_ref::<Callback>().is_some()
                {
                    let ptr = Arc::as_ptr(payload) as *const () as usize;
                    if seen.insert(ptr)
                        && let Ok(cb_arc) = payload.clone().downcast::<Callback>()
                    {
                        prepare_list.push((ptr, cb_arc));
                    }
                }
            }
            if !prepare_list.is_empty() {
                let mut callback_targets: HashMap<usize, Vec<(PassTarget, ScreenDescriptor)>> =
                    HashMap::new();
                for pass in &passes {
                    let (size, target_format, sample_count) = match pass.target {
                        PassTarget::Surface => (
                            [self.output_width, self.output_height],
                            if self.working_space {
                                wgpu::TextureFormat::Rgba16Float
                            } else {
                                self.output_format
                            },
                            self.active_surface_msaa_samples(),
                        ),
                        PassTarget::Layer(layer_id) => {
                            let layer = self.layer_pool.get(&layer_id);
                            (
                                layer.map_or([self.output_width, self.output_height], |layer| {
                                    [layer.width, layer.height]
                                }),
                                self.layer_target_format(),
                                1,
                            )
                        }
                    };
                    for command in &pass.cmds {
                        if let Cmd::Callback { payload, .. } = command {
                            let descriptor = ScreenDescriptor {
                                size_in_pixels: size,
                                pixels_per_point: self.pixels_per_point,
                                target_format,
                                sample_count,
                            };
                            let targets = callback_targets
                                .entry(Arc::as_ptr(payload) as *const () as usize)
                                .or_default();
                            if !targets.iter().any(|(target, target_desc)| {
                                *target == pass.target
                                    && target_desc.size_in_pixels == descriptor.size_in_pixels
                                    && target_desc.pixels_per_point == descriptor.pixels_per_point
                                    && target_desc.target_format == descriptor.target_format
                                    && target_desc.sample_count == descriptor.sample_count
                            }) {
                                targets.push((pass.target, descriptor));
                            }
                        }
                    }
                }

                let default_descriptor = ScreenDescriptor {
                    size_in_pixels: [self.output_width, self.output_height],
                    pixels_per_point: self.pixels_per_point,
                    target_format: if self.working_space {
                        wgpu::TextureFormat::Rgba16Float
                    } else {
                        self.output_format
                    },
                    sample_count: self.active_surface_msaa_samples(),
                };
                let mut prepare_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("callback prepare"),
                        });
                let mut finish_encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("callback finish prepare"),
                        });
                let mut active_callback_scopes = HashSet::new();
                for (key, cb) in &prepare_list {
                    let descriptors = callback_targets
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| vec![(PassTarget::Surface, default_descriptor)]);
                    for (target, screen_desc) in descriptors {
                        let scope = CallbackScopeKey {
                            callback: *key,
                            target,
                            width: screen_desc.size_in_pixels[0],
                            height: screen_desc.size_in_pixels[1],
                            target_format: screen_desc.target_format,
                            sample_count: screen_desc.sample_count,
                            pixels_per_point_bits: screen_desc.pixels_per_point.to_bits(),
                        };
                        active_callback_scopes.insert(scope);
                        if self
                            .callback_scope_payloads
                            .get(&scope)
                            .is_some_and(|payload| payload.upgrade().is_none())
                        {
                            self.remove_callback_scope(&scope);
                        }
                        let payload: Weak<Callback> = Arc::downgrade(cb);
                        self.callback_scope_payloads.insert(scope, payload);
                    }
                }
                let mut prepare_buffers = Vec::new();
                let mut finish_buffers = Vec::new();
                for (key, cb) in &prepare_list {
                    let descriptors = callback_targets
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| vec![(PassTarget::Surface, default_descriptor)]);
                    for (target, screen_desc) in descriptors {
                        let scope = CallbackScopeKey {
                            callback: *key,
                            target,
                            width: screen_desc.size_in_pixels[0],
                            height: screen_desc.size_in_pixels[1],
                            target_format: screen_desc.target_format,
                            sample_count: screen_desc.sample_count,
                            pixels_per_point_bits: screen_desc.pixels_per_point.to_bits(),
                        };
                        self.touch_callback_scope(scope);
                        if !self.callback_scoped_resources.contains_key(&scope)
                            && self.callback_scoped_resources.len() >= MAX_CALLBACK_SCOPES
                        {
                            self.evict_callback_scope(&active_callback_scopes);
                        }
                        let resources = self.callback_scoped_resources.entry(scope).or_default();
                        prepare_buffers.extend(cb.0.prepare(
                            &self.device,
                            &self.queue,
                            &mut prepare_encoder,
                            &screen_desc,
                            resources,
                        ));
                    }
                }
                for (key, cb) in &prepare_list {
                    let descriptors = callback_targets
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| vec![(PassTarget::Surface, default_descriptor)]);
                    for (target, screen_desc) in descriptors {
                        let scope = CallbackScopeKey {
                            callback: *key,
                            target,
                            width: screen_desc.size_in_pixels[0],
                            height: screen_desc.size_in_pixels[1],
                            target_format: screen_desc.target_format,
                            sample_count: screen_desc.sample_count,
                            pixels_per_point_bits: screen_desc.pixels_per_point.to_bits(),
                        };
                        self.touch_callback_scope(scope);
                        if let Some(resources) = self.callback_scoped_resources.get_mut(&scope) {
                            finish_buffers.extend(cb.0.finish_prepare(
                                &self.device,
                                &self.queue,
                                &mut finish_encoder,
                                &screen_desc,
                                resources,
                            ));
                        }
                    }
                }
                let mut callback_buffers =
                    Vec::with_capacity(2 + prepare_buffers.len() + finish_buffers.len());
                callback_buffers.push(prepare_encoder.finish());
                callback_buffers.extend(prepare_buffers);
                callback_buffers.push(finish_encoder.finish());
                callback_buffers.extend(finish_buffers);
                self.queue.submit(callback_buffers);
            }
        }

        let globals_bytes = std::mem::size_of::<Globals>() as u64;
        let globals_staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals staging"),
            size: (passes.len().max(1) as u64) * globals_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        for (i, pass) in passes.iter().enumerate() {
            let (target_w, target_h) = match pass.target {
                PassTarget::Surface => (fb_w, fb_h),
                PassTarget::Layer(layer_id) => {
                    let lt = self.layer_pool.get(&layer_id);
                    (
                        lt.map_or(fb_w, |l| l.width as f32),
                        lt.map_or(fb_h, |l| l.height as f32),
                    )
                }
            };
            self.queue.write_buffer(
                &globals_staging,
                (i as u64) * globals_bytes,
                bytemuck::bytes_of(&make_globals(target_w, target_h)),
            );
        }

        let bind_mask = self.atlas_bind_group_mask();
        let bind_color = self.atlas_bind_group_color();
        let mut clip_depth: u32 = 0;
        let mut clip_depth_stack: Vec<u32> = Vec::new();

        let snapshot_source = target_texture.cloned();
        for (pass_index, pass) in std::mem::take(&mut passes).into_iter().enumerate() {
            // Populate backdrop snapshots for blends composited in this
            // pass: copy the parent target region into the snapshot
            // texture. Passes execute in order, so the parent holds the
            // true backdrop. Surface parents need the target texture (no
            // swapchain sampling mid-frame); layer parents copy from the
            // pool. Either way the region is parent-target pixels. Copies
            // for other passes stay queued (`remaining`).
            let needs_snapshot = pass
                .cmds
                .iter()
                .any(|c| matches!(c, Cmd::BlendLayer { .. }));
            if needs_snapshot {
                let copies = std::mem::take(&mut self.blend_copies);
                let has_surface_copy = copies.iter().any(|copy| copy.target == PassTarget::Surface);
                let mut resolved_surface = None;
                if has_surface_copy
                    && !self.working_space
                    && let (Some(msaa_view), Some(resolve_view)) =
                        (self.msaa_view.as_ref(), self.surface_resolve_view.as_ref())
                {
                    let resolve_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("surface blend resolve"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: msaa_view,
                            resolve_target: Some(resolve_view),
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    drop(resolve_pass);
                    resolved_surface = self.surface_resolve_tex.clone();
                }
                let mut remaining = Vec::with_capacity(copies.len());
                for copy in copies {
                    if copy.pass_id != pass.id {
                        remaining.push(copy);
                        continue;
                    }
                    let BlendCopy {
                        blend_id,
                        target,
                        region,
                        ..
                    } = copy;
                    let (src_tex, tw, th) = match target {
                        PassTarget::Layer(parent_id) => match self.layer_pool.get(&parent_id) {
                            Some(lt) => (lt.texture.clone(), lt.width, lt.height),
                            None => continue,
                        },
                        PassTarget::Surface => {
                            let texture = if self.working_space {
                                self.ws_tex.clone()
                            } else if self.msaa_view.is_some() {
                                resolved_surface.clone()
                            } else {
                                snapshot_source.clone()
                            };
                            match texture {
                                Some(texture) => (texture, self.output_width, self.output_height),
                                None => {
                                    self.remove_blend_snapshot(blend_id);
                                    continue;
                                }
                            }
                        }
                    };
                    let Some(dst_tex) = self.blend_snapshot_texture(blend_id) else {
                        continue;
                    };
                    if !src_tex.usage().contains(wgpu::TextureUsages::COPY_SRC)
                        || !dst_tex.usage().contains(wgpu::TextureUsages::COPY_DST)
                    {
                        self.remove_blend_snapshot(blend_id);
                        continue;
                    }
                    let Some((sx, sy, cw, ch)) = checked_copy_region(region, tw, th) else {
                        self.remove_blend_snapshot(blend_id);
                        continue;
                    };
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &src_tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d { x: sx, y: sy, z: 0 },
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &dst_tex,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: cw,
                            height: ch,
                            depth_or_array_layers: 1,
                        },
                    );
                }
                self.blend_copies = remaining;
            }
            let (color_view, resolve_target, depth_stencil_view, is_layer, working_target) =
                match pass.target {
                    PassTarget::Surface => {
                        let swap_view = target_view.clone();
                        let use_ws = self.working_space && self.ws_view.is_some();
                        let (color, resolve) = if use_ws {
                            let ws_view = self.ws_view.as_ref().unwrap();
                            if let Some(msaa_view) = &self.ws_msaa_view {
                                (msaa_view.clone(), Some(ws_view.clone()))
                            } else {
                                (ws_view.clone(), None)
                            }
                        } else if let Some(msaa_view) = &self.msaa_view {
                            (msaa_view.clone(), Some(swap_view))
                        } else {
                            (swap_view, None)
                        };
                        (
                            color,
                            resolve,
                            self.depth_stencil_view.clone(),
                            false,
                            use_ws,
                        )
                    }
                    PassTarget::Layer(layer_id) => {
                        if let Some(lt) = self.layer_pool.get(&layer_id) {
                            (
                                lt.view.clone(),
                                None,
                                lt.depth_stencil_view.clone(),
                                true,
                                false,
                            )
                        } else {
                            log::warn!("missing layer target {layer_id}");
                            continue;
                        }
                    }
                };

            encoder.copy_buffer_to_buffer(
                &globals_staging,
                (pass_index as u64) * globals_bytes,
                &self.globals_buf,
                0,
                globals_bytes,
            );

            if is_layer {
                clip_depth_stack.push(clip_depth);
                clip_depth = 0;
            }

            let (tw, th) = match pass.target {
                PassTarget::Surface => (self.output_width, self.output_height),
                PassTarget::Layer(layer_id) => self
                    .layer_pool
                    .get(&layer_id)
                    .map(|l| (l.width, l.height))
                    .unwrap_or((self.output_width, self.output_height)),
            };
            let initial_scissor = clamp_scissor(
                pass.initial_scissor.0,
                pass.initial_scissor.1,
                pass.initial_scissor.2,
                pass.initial_scissor.3,
                tw,
                th,
            );
            let mut active_scissor = pass
                .active_scissor
                .map(|scissor| clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th));
            if active_scissor.is_some_and(|scissor| scissor.2 == 0 || scissor.3 == 0) {
                active_scissor = None;
            }

            let pipes: &Pipelines = if is_layer {
                if self.working_space {
                    &self.working_space_layer_pipes
                } else {
                    &self.layer_pipes
                }
            } else if working_target {
                &self.working_space_pipes
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
                        load: if pass.clear_color.is_some() {
                            wgpu::LoadOp::Clear(STENCIL_BASE)
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
            rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
            let initial_draw_scissor = if initial_scissor.2 == 0 || initial_scissor.3 == 0 {
                (0, 0, 1, 1)
            } else {
                initial_scissor
            };
            rpass.set_scissor_rect(
                initial_draw_scissor.0,
                initial_draw_scissor.1,
                initial_draw_scissor.2,
                initial_draw_scissor.3,
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

            macro_rules! draw_indexed_mesh {
                ($pipeline:expr, $uoff:ident, $voff:ident, $vcnt:ident, $ioff:ident, $icnt:ident) => {{
                    rpass.set_pipeline($pipeline);
                    rpass.set_bind_group(1, &self.mesh_bind, &[$uoff as u32]);
                    let vbytes = ($vcnt as u64) * std::mem::size_of::<MeshVertex>() as u64;
                    rpass.set_vertex_buffer(0, self.mesh_verts.buf.slice($voff..$voff + vbytes));
                    let ibytes = ($icnt as u64) * std::mem::size_of::<u32>() as u64;
                    rpass.set_index_buffer(
                        self.mesh_indices.buf.slice($ioff..$ioff + ibytes),
                        wgpu::IndexFormat::Uint32,
                    );
                    rpass.draw_indexed(0..$icnt, 0, 0..1);
                }};
            }

            for cmd in pass.cmds {
                if active_scissor.is_none()
                    && !matches!(
                        &cmd,
                        Cmd::ClipPush { .. }
                            | Cmd::ClipPop { .. }
                            | Cmd::VectorClipPush { .. }
                            | Cmd::VectorClipPop { .. }
                    )
                {
                    continue;
                }
                match cmd {
                    Cmd::ClipPush {
                        off,
                        cnt: n,
                        scissor,
                        difference,
                        applied,
                    } => {
                        let scissor =
                            clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th);
                        let usable = applied && scissor.2 > 0 && scissor.3 > 0;
                        if usable {
                            rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                            active_scissor = Some(scissor);
                            rpass.set_pipeline(if difference {
                                &pipes.clip_dec
                            } else {
                                &pipes.clip_bin
                            });

                            let bytes = (n as u64) * std::mem::size_of::<ClipInstance>() as u64;
                            rpass.set_vertex_buffer(0, self.clip_ring.buf.slice(off..off + bytes));
                            rpass.draw(0..6, 0..n);
                            if !difference {
                                clip_depth = (clip_depth + 1).min(STENCIL_MAX_DEPTH);
                            }
                        } else {
                            active_scissor = None;
                        }
                        rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
                    }

                    Cmd::ClipPop {
                        off,
                        cnt: n,
                        scissor,
                        difference,
                        applied,
                    } => {
                        let scissor =
                            clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th);
                        if scissor.2 > 0 && scissor.3 > 0 {
                            rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                            active_scissor = Some(scissor);
                        } else {
                            active_scissor = None;
                        }
                        if applied && n > 0 && scissor.2 > 0 && scissor.3 > 0 {
                            rpass.set_pipeline(if difference {
                                &pipes.clip_bin
                            } else {
                                &pipes.clip_dec
                            });
                            let bytes = (n as u64) * std::mem::size_of::<ClipInstance>() as u64;
                            rpass.set_vertex_buffer(0, self.clip_ring.buf.slice(off..off + bytes));
                            rpass.draw(0..6, 0..n);
                            if !difference {
                                clip_depth = clip_depth.saturating_sub(1);
                            }
                        }
                        rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
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

                    Cmd::GlyphsVector { off, cnt: n } => {
                        if let Some(slug_pipe) = pipes.slug.as_ref() {
                            rpass.set_pipeline(slug_pipe);
                            let bytes = (n as u64) * std::mem::size_of::<slug::TessVertex>() as u64;
                            rpass.set_vertex_buffer(0, self.slug_ring.buf.slice(off..off + bytes));
                            rpass.draw(0..n, 0..1);
                        }
                    }

                    Cmd::ImageRgba {
                        off,
                        cnt: n,
                        handle,
                    } => {
                        let bind_opt = match self.images.get(&handle) {
                            Some(ImageTex::Rgba { bind, .. }) => Some(bind),
                            Some(ImageTex::User { bind, .. }) => Some(bind),
                            _ => None,
                        };
                        if let Some(bind) = bind_opt {
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
                    Cmd::Coverage {
                        off,
                        cnt: n,
                        handle,
                    } => {
                        if let Some(tile) = self.coverages.get(&handle) {
                            draw_with_bind!(
                                &pipes.coverage,
                                self.glyph_color.ring,
                                GlyphInstance,
                                &tile.bind,
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

                    Cmd::Arc { off, cnt: n } => {
                        draw_simple!(&pipes.arcs, self.arcs.ring, ArcInstance, off, n);
                    }

                    Cmd::CompositeLayer {
                        off,
                        cnt: n,
                        layer_id,
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
                                &lt.bind_linear,
                                off,
                                n
                            );
                        }
                    }
                    Cmd::CompositeBlur {
                        off,
                        cnt: n,
                        layer_id,
                    } => {
                        if let Some(lt) = self.layer_pool.get(&layer_id).cloned() {
                            draw_with_bind!(
                                &pipes.blur_content,
                                self.blur_ring,
                                BlurInstance,
                                &lt.bind_linear,
                                off,
                                n
                            );
                        }
                    }
                    Cmd::CompositeProjective {
                        off,
                        cnt: n,
                        layer_id,
                    } => {
                        if let Some(lt) = self.layer_pool.get(&layer_id).cloned() {
                            // The layer texture is sampled with the rgba
                            // (non-linear-filter) binding, like the sharp
                            // composite path.
                            draw_with_bind!(
                                &pipes.projective_layer,
                                self.projective_ring,
                                ProjectiveInstance,
                                &lt.bind,
                                off,
                                n
                            );
                        }
                    }

                    Cmd::BlendLayer {
                        off,
                        cnt: n,
                        src_layer,
                        dst_layer,
                        parent,
                        scissor,
                    } => {
                        let src = self.layer_pool.get(&src_layer).cloned();
                        // Backdrop is the snapshot copied from the parent
                        // target before this pass opened (a texture cannot be
                        // sampled mid-pass while bound as a render target).
                        // A missing snapshot (surface parent without texture,
                        // evicted layer) skips the draw: the mesh vanishes
                        // rather than misrendering.
                        let dst = dst_layer.and_then(|id| self.blend_snapshots.get(&id).cloned());
                        let in_parent = match parent {
                            PassTarget::Surface => !is_layer,
                            PassTarget::Layer(id) => {
                                matches!(pass.target, PassTarget::Layer(pid) if pid == id)
                            }
                        };
                        if let (Some(src_lt), Some(dst_snap)) = (src, dst)
                            && in_parent
                        {
                            let bytes = (n as u64) * std::mem::size_of::<BlendInstance>() as u64;
                            rpass.set_pipeline(&pipes.blend_layer);
                            let scissor =
                                clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th);
                            if scissor.2 > 0 && scissor.3 > 0 {
                                rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                                rpass.set_bind_group(0, &self.globals_bind, &[]);
                                rpass.set_bind_group(1, &src_lt.bind, &[]);
                                rpass.set_bind_group(2, &dst_snap.bind, &[]);
                                rpass.set_vertex_buffer(
                                    0,
                                    self.blend_ring.buf.slice(off..off + bytes),
                                );
                                rpass.draw(0..6, 0..n);
                            }
                        }
                    }

                    Cmd::VectorMesh {
                        voff,
                        vcnt,
                        ioff,
                        icnt,
                        uoff,
                        blend,
                    } => {
                        let pipe = match blend {
                            repose_core::BlendMode::Add => &pipes.mesh_add,
                            repose_core::BlendMode::Multiply => &pipes.mesh_multiply,
                            repose_core::BlendMode::Screen => &pipes.mesh_screen,
                            repose_core::BlendMode::Darken => &pipes.mesh_darken,
                            repose_core::BlendMode::Lighten => &pipes.mesh_lighten,
                            _ => &pipes.mesh,
                        };
                        draw_indexed_mesh!(pipe, uoff, voff, vcnt, ioff, icnt);
                    }

                    Cmd::VectorOverlay {
                        voff,
                        vcnt,
                        ioff,
                        icnt,
                        uoff,
                    } => {
                        draw_indexed_mesh!(&pipes.mesh_overlay, uoff, voff, vcnt, ioff, icnt);
                    }

                    Cmd::VectorClipPush {
                        voff,
                        vcnt,
                        ioff,
                        icnt,
                        uoff,
                        scissor,
                        difference,
                        applied,
                    } => {
                        let scissor =
                            clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th);
                        let usable = applied && scissor.2 > 0 && scissor.3 > 0;
                        if usable {
                            rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                            active_scissor = Some(scissor);
                            let pipe = if difference {
                                &pipes.mesh_clip_dec
                            } else {
                                &pipes.mesh_clip_inc
                            };
                            draw_indexed_mesh!(pipe, uoff, voff, vcnt, ioff, icnt);
                            if !difference {
                                clip_depth = (clip_depth + 1).min(STENCIL_MAX_DEPTH);
                            }
                        } else {
                            active_scissor = None;
                        }
                        rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
                    }

                    Cmd::VectorClipPop {
                        voff,
                        vcnt,
                        ioff,
                        icnt,
                        uoff,
                        scissor,
                        difference,
                        applied,
                    } => {
                        let scissor =
                            clamp_scissor(scissor.0, scissor.1, scissor.2, scissor.3, tw, th);
                        if scissor.2 > 0 && scissor.3 > 0 {
                            rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                            active_scissor = Some(scissor);
                        } else {
                            active_scissor = None;
                        }
                        if applied && vcnt > 0 && scissor.2 > 0 && scissor.3 > 0 {
                            let pipe = if difference {
                                &pipes.mesh_clip_inc
                            } else {
                                &pipes.mesh_clip_dec
                            };
                            draw_indexed_mesh!(pipe, uoff, voff, vcnt, ioff, icnt);
                            if !difference {
                                clip_depth = clip_depth.saturating_sub(1);
                            }
                        }
                        rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
                    }

                    Cmd::Callback {
                        rect,
                        clip_rect,
                        scissor,
                        restore_scissor,
                        callback_id,
                        payload,
                    } => {
                        if let Some(cb) = payload.downcast_ref::<Callback>() {
                            let vp_x = rect.x.floor().max(0.0);
                            let vp_y = rect.y.floor().max(0.0);
                            let vp_w = rect.w.ceil().max(1.0);
                            let vp_h = rect.h.ceil().max(1.0);
                            if vp_w > 0.0 && vp_h > 0.0 && clip_rect.w > 0.0 && clip_rect.h > 0.0 {
                                rpass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                                rpass.set_viewport(vp_x, vp_y, vp_w, vp_h, 0.0, 1.0);
                                let info = repose_core::PaintCallbackInfo {
                                    viewport: rect,
                                    clip_rect,
                                    pixels_per_point: self.pixels_per_point,
                                    screen_size_px: [tw, th],
                                };
                                let descriptor = ScreenDescriptor {
                                    size_in_pixels: [tw, th],
                                    pixels_per_point: self.pixels_per_point,
                                    target_format: if is_layer {
                                        self.layer_target_format()
                                    } else if working_target {
                                        wgpu::TextureFormat::Rgba16Float
                                    } else {
                                        self.output_format
                                    },
                                    sample_count: if is_layer {
                                        1
                                    } else {
                                        self.active_surface_msaa_samples()
                                    },
                                };
                                let mut scope =
                                    callback_scope_key(&payload, pass.target, &descriptor);
                                scope.callback = callback_id;
                                let resources = self
                                    .callback_scoped_resources
                                    .get(&scope)
                                    .unwrap_or(&self.callback_resources);
                                let mut callback_pass = CallbackRenderPass::new(
                                    &mut rpass,
                                    descriptor.target_format,
                                    descriptor.sample_count,
                                );
                                cb.0.paint(info, &mut callback_pass, resources);
                                rpass.set_viewport(0.0, 0.0, tw as f32, th as f32, 0.0, 1.0);
                                let restore = if restore_scissor.2 > 0 && restore_scissor.3 > 0 {
                                    restore_scissor
                                } else {
                                    (0, 0, 1, 1)
                                };
                                rpass.set_scissor_rect(restore.0, restore.1, restore.2, restore.3);
                                rpass.set_bind_group(0, &self.globals_bind, &[]);
                                rpass.set_stencil_reference(STENCIL_BASE + clip_depth);
                            }
                        } else {
                            log::warn!("Unknown paint callback payload");
                        }
                    }
                }
            }
            if is_layer {
                clip_depth = clip_depth_stack.pop().unwrap_or(0);
            }
        }

        // frame's ids so the next translation drains their textures.
        self.flatten_layer_ids = flatten_ids_used;

        // Display pass: linear working space -> sRGB OETF -> swapchain
        if self.working_space
            && let (Some(_ws_view), Some(ws_bind), Some(display_pipeline)) =
                (&self.ws_view, &self.ws_bind, &self.display_pipeline)
        {
            let swap_view = target_view.clone();
            let mut display_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("display transform"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &swap_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            display_pass.set_pipeline(display_pipeline);
            display_pass.set_bind_group(1, ws_bind, &[]);
            display_pass.draw(0..3, 0..1);
        }
    }

    /// Render a scene into an externally-provided texture view.
    /// Use this when embedding Repose in a host that owns the GPU.
    /// The host is responsible for submitting the encoder and handling present.
    pub fn render_to_view(
        &mut self,
        scene: &Scene,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        clear_color: Option<[f64; 4]>,
    ) {
        self.resize(width, height);

        if let Err(error) = self.begin_frame() {
            log::error!("render_to_view: {error:#}");
            return;
        }

        if width == 0 || height == 0 {
            self.end_frame();
            return;
        }

        self.render_scene_to_encoder(scene, encoder, target_view, clear_color);
        self.end_frame();
    }
}

#[cfg(target_os = "linux")]
fn validate_dmabuf_plane(
    fd: &std::os::unix::io::OwnedFd,
    offset: u64,
    stride: u32,
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
) -> anyhow::Result<u64> {
    if width == 0 || height == 0 || bytes_per_pixel == 0 {
        anyhow::bail!("DMA-BUF plane dimensions and bytes per pixel must be non-zero");
    }
    if offset % 4 != 0 || stride % 4 != 0 {
        anyhow::bail!("DMA-BUF offset and stride must be 4-byte aligned");
    }
    let row_bytes = (width as u64)
        .checked_mul(bytes_per_pixel as u64)
        .ok_or_else(|| anyhow::anyhow!("DMA-BUF row size overflow"))?;
    if (stride as u64) < row_bytes {
        anyhow::bail!(
            "DMA-BUF stride {} is smaller than row size {row_bytes}",
            stride
        );
    }
    let span = (height as u64 - 1)
        .checked_mul(stride as u64)
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| anyhow::anyhow!("DMA-BUF plane size overflow"))?;
    let file = std::fs::File::from(
        fd.try_clone()
            .map_err(|error| anyhow::anyhow!("duplicate DMA-BUF fd: {error}"))?,
    );
    let file_size = file
        .metadata()
        .map_err(|error| anyhow::anyhow!("stat DMA-BUF fd: {error}"))?
        .len();
    if offset > file_size || span > file_size - offset {
        anyhow::bail!("DMA-BUF plane offset/size {offset}+{span} exceeds file size {file_size}");
    }
    Ok(file_size)
}

fn is_filterable_color_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::Rgba16Unorm
            | wgpu::TextureFormat::Rgba16Float
            | wgpu::TextureFormat::Rgba32Float
            | wgpu::TextureFormat::Rgb10a2Unorm
    )
}

fn native_format_supported(format: wgpu::TextureFormat, features: wgpu::Features) -> bool {
    if !is_filterable_color_format(format) {
        return false;
    }
    match format {
        wgpu::TextureFormat::Rgba16Unorm => {
            features.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
        }
        wgpu::TextureFormat::Rgba32Float => features.contains(wgpu::Features::FLOAT32_FILTERABLE),
        _ => true,
    }
}

fn validate_sampler_descriptor(
    device: &wgpu::Device,
    sampler: &wgpu::SamplerDescriptor<'_>,
) -> anyhow::Result<()> {
    if sampler.compare.is_some()
        || !matches!(
            sampler.mag_filter,
            wgpu::FilterMode::Nearest | wgpu::FilterMode::Linear
        )
        || !matches!(
            sampler.min_filter,
            wgpu::FilterMode::Nearest | wgpu::FilterMode::Linear
        )
        || sampler.anisotropy_clamp == 0
        || !sampler.lod_min_clamp.is_finite()
        || !sampler.lod_max_clamp.is_finite()
        || sampler.lod_min_clamp < 0.0
        || sampler.lod_max_clamp < sampler.lod_min_clamp
    {
        anyhow::bail!("native sampler must be non-comparison linear filtering with finite LODs");
    }
    if matches!(sampler.address_mode_u, wgpu::AddressMode::ClampToBorder)
        || matches!(sampler.address_mode_v, wgpu::AddressMode::ClampToBorder)
        || matches!(sampler.address_mode_w, wgpu::AddressMode::ClampToBorder)
    {
        if !device
            .features()
            .contains(wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER)
        {
            anyhow::bail!("native sampler uses unsupported clamp-to-border addressing");
        }
    }
    Ok(())
}

fn validate_texture_dimensions(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    if width == 0
        || height == 0
        || width > device.limits().max_texture_dimension_2d
        || height > device.limits().max_texture_dimension_2d
    {
        anyhow::bail!("texture dimensions are outside the device limit");
    }
    Ok(())
}

fn validate_image_handle(handle: u64) -> anyhow::Result<()> {
    if handle == 0 {
        anyhow::bail!("image handle 0 is reserved");
    }
    Ok(())
}

fn checked_image_bytes(w: u32, h: u32, bytes_per_pixel: u32) -> anyhow::Result<u64> {
    if w == 0 || h == 0 || bytes_per_pixel == 0 {
        anyhow::bail!("image dimensions and bytes per pixel must be non-zero");
    }
    (w as u64)
        .checked_mul(h as u64)
        .and_then(|size| size.checked_mul(bytes_per_pixel as u64))
        .ok_or_else(|| anyhow::anyhow!("image byte size overflow"))
}

fn texture_storage_bytes(
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> anyhow::Result<u64> {
    if width == 0 || height == 0 {
        anyhow::bail!("texture dimensions must be non-zero");
    }
    let bytes = format.theoretical_memory_footprint(wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    });
    if bytes == 0 {
        anyhow::bail!("texture format has no storage size");
    }
    Ok(bytes)
}

fn texture_storage_bytes_with_samples(
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> anyhow::Result<u64> {
    texture_storage_bytes(format, width, height)?
        .checked_mul(u64::from(sample_count.max(1)))
        .ok_or_else(|| anyhow::anyhow!("multisample texture size overflow"))
}

fn checked_image_bytes_usize(w: u32, h: u32, bytes_per_pixel: u32) -> anyhow::Result<usize> {
    usize::try_from(checked_image_bytes(w, h, bytes_per_pixel)?)
        .map_err(|_| anyhow::anyhow!("image byte size exceeds addressable memory"))
}

fn clamp_scissor(x: u32, y: u32, w: u32, h: u32, tw: u32, th: u32) -> (u32, u32, u32, u32) {
    if w == 0 || h == 0 || tw == 0 || th == 0 {
        return (0, 0, 0, 0);
    }
    let x = x.min(tw - 1);
    let y = y.min(th - 1);
    let w = w.min(tw - x);
    let h = h.min(th - y);
    (x, y, w, h)
}

fn rect_to_scissor(r: repose_core::Rect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    if width == 0
        || height == 0
        || !r.x.is_finite()
        || !r.y.is_finite()
        || !r.w.is_finite()
        || !r.h.is_finite()
        || r.w <= 0.0
        || r.h <= 0.0
    {
        return (0, 0, 0, 0);
    }
    let x0 = r.x.floor().max(0.0).min(width as f32);
    let y0 = r.y.floor().max(0.0).min(height as f32);
    let x1 = (r.x + r.w).ceil().max(x0).min(width as f32);
    let y1 = (r.y + r.h).ceil().max(y0).min(height as f32);
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, 0, 0);
    }
    (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
}

fn rect_to_ndc(r: repose_core::Rect, width: f32, height: f32) -> [f32; 4] {
    if width <= 0.0 || height <= 0.0 || !r.x.is_finite() || !r.y.is_finite() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let x0 = r.x / width * 2.0 - 1.0;
    let y0 = 1.0 - r.y / height * 2.0;
    let x1 = (r.x + r.w) / width * 2.0 - 1.0;
    let y1 = 1.0 - (r.y + r.h) / height * 2.0;
    [x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()]
}

fn translated_rect(rect: repose_core::Rect, x: f32, y: f32) -> repose_core::Rect {
    repose_core::Rect {
        x: rect.x - x,
        y: rect.y - y,
        w: rect.w,
        h: rect.h,
    }
}

fn checked_copy_region(
    region: repose_core::Rect,
    target_width: u32,
    target_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if target_width == 0
        || target_height == 0
        || !region.x.is_finite()
        || !region.y.is_finite()
        || !region.w.is_finite()
        || !region.h.is_finite()
        || region.w <= 0.0
        || region.h <= 0.0
    {
        return None;
    }
    let x0 = region.x.floor().max(0.0).min(target_width as f32) as u32;
    let y0 = region.y.floor().max(0.0).min(target_height as f32) as u32;
    let x1 = (region.x + region.w)
        .ceil()
        .max(x0 as f32)
        .min(target_width as f32) as u32;
    let y1 = (region.y + region.h)
        .ceil()
        .max(y0 as f32)
        .min(target_height as f32) as u32;
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_alignment_rejects_bad_input() {
        assert_eq!(align_up(0, 4).unwrap(), 0);
        assert_eq!(align_up(1, 4).unwrap(), 4);
        assert!(align_up(1, 3).is_err());
        assert!(align_up(u64::MAX, 4).is_err());
    }

    #[test]
    fn empty_and_negative_rects_are_suppressed() {
        assert_eq!(
            rect_to_scissor(
                repose_core::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 4.0,
                },
                16,
                16,
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(
            rect_to_scissor(
                repose_core::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: -1.0,
                    h: 4.0,
                },
                16,
                16,
            ),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn copy_region_is_checked() {
        assert_eq!(
            checked_copy_region(
                repose_core::Rect {
                    x: 2.25,
                    y: 3.5,
                    w: 4.0,
                    h: 5.0,
                },
                16,
                16,
            ),
            Some((2, 3, 5, 6))
        );
        assert_eq!(
            checked_copy_region(
                repose_core::Rect {
                    x: 20.0,
                    y: 0.0,
                    w: 2.0,
                    h: 2.0,
                },
                16,
                16,
            ),
            None
        );
    }

    #[test]
    fn p010_uses_ten_bit_sample_range() {
        let raw = make_yuv_transform_raw(ColorInfo::default(), PixelFormat::P010);
        let expected = 65535.0 / (64.0 * 1020.0);
        assert!((raw.b[3] - expected).abs() < 1e-6);
        assert_eq!(canonical_fourcc(PixelFormat::P010), P010_FOURCC);
        assert_eq!(canonical_fourcc(PixelFormat::Nv12), NV12_FOURCC);
        assert_eq!(
            make_yuv_transform_raw(
                ColorInfo {
                    transfer: repose_core::color::Transfer::Pq,
                    ..ColorInfo::default()
                },
                PixelFormat::Nv12,
            )
            .transfer[0],
            3.0
        );
        assert_eq!(
            make_yuv_transform_raw(
                ColorInfo {
                    transfer: repose_core::color::Transfer::Hlg,
                    ..ColorInfo::default()
                },
                PixelFormat::Nv12,
            )
            .transfer[0],
            4.0
        );
    }
}
