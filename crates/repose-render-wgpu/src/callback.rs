//! The core crate stores `Arc<dyn Any>` in `SceneNode::Callback`; this crate
//! provides the concrete `Callback` wrapper and the `WgpuCallback` trait that
//! the renderer downcasts to.
//!
//! ```ignore
//! use repose_core::prelude::*; // remember_mutable, request_frame, Modifier, View, PaintCallbackInfo
//! use repose_render_wgpu::{Callback, WgpuCallback, CallbackResources};
//! use repose_ui::Embedded; // or repose_canvas::Embedded
//!
//! struct MyTriangle { angle: f32 }
//! impl WgpuCallback for MyTriangle {
//!     fn prepare(&self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, screen: &repose_render_wgpu::ScreenDescriptor, resources: &mut CallbackResources) {
//!         // resources.get_or_insert_with::<Pipelines>() -> update uniform buffer with self.angle
//!     }
//!     fn paint(&self, info: PaintCallbackInfo, rpass: &mut repose_render_wgpu::CallbackRenderPass<'_>, resources: &CallbackResources) {
//!         // info.viewport is layout rect (physical px), renderer already set viewport -> info.viewport
//!         // resources.get::<Pipelines>().unwrap().paint(rpass)
//!     }
//! }
//!
//! fn MyView() -> View {
//!     let angle = remember_mutable(|| 0f32);
//!     // Signal is !Send -> snapshot Copy value into callback (not the Signal itself)
//!     let payload = { let a = *angle.get(); Callback::new(MyTriangle{ angle: a }) };
//!     Embedded(
//!         Modifier::new().size(300.0,300.0)
//!             .on_pointer_move({ let a=angle.clone(); move |ev: PointerEvent| { a.update(|v| *v+=ev.position.x*0.01); request_frame() } }),
//!         payload,
//!     )
//! }
//! // For offscreen textures (bevy render target) use register_native_texture -> Image.
//! ```

use std::any::TypeId;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use repose_core::{PaintCallbackInfo, PaintCallbackPayload, Rect};

/// Type-map for callback-shared wgpu resources (pipelines, buffers, etc.).
#[cfg(not(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
)))]
type AnyBox = Box<dyn std::any::Any + Send + Sync>;
#[cfg(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
))]
type AnyBox = Box<dyn std::any::Any>;

#[cfg(not(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
)))]
pub trait MaybeSendSync: Send + Sync + 'static {}
#[cfg(not(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
)))]
impl<T: Send + Sync + 'static> MaybeSendSync for T {}

#[cfg(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
))]
pub trait MaybeSendSync: 'static {}
#[cfg(all(
    target_arch = "wasm32",
    any(
        not(feature = "fragile-send-sync-non-atomic-wasm"),
        target_feature = "atomics"
    )
))]
impl<T: 'static> MaybeSendSync for T {}

#[derive(Default)]
pub struct CallbackResources {
    map: HashMap<TypeId, AnyBox>,
}

impl CallbackResources {
    pub fn insert<T: MaybeSendSync>(&mut self, value: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: MaybeSendSync>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn get_mut<T: MaybeSendSync>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut::<T>())
    }

    pub fn get_or_insert_with<T: MaybeSendSync + Default>(&mut self) -> &mut T {
        let id = TypeId::of::<T>();
        self.map.entry(id).or_insert_with(|| Box::new(T::default()));
        self.map.get_mut(&id).unwrap().downcast_mut::<T>().unwrap()
    }

    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|b| b.downcast::<T>().ok())
            .map(|b| *b)
    }

    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScreenDescriptor {
    pub size_in_pixels: [u32; 2],
    pub pixels_per_point: f32,
    /// Target surface format (e.g. `Bgra8UnormSrgb`), so `prepare` can create pipelines.
    pub target_format: wgpu::TextureFormat,
    /// MSAA sample count of the surface render pass (1 or 4).
    pub sample_count: u32,
}

pub struct CallbackRenderPass<'pass, 'encoder> {
    pass: &'pass mut wgpu::RenderPass<'encoder>,
    target_format: wgpu::TextureFormat,
    sample_count: u32,
}

impl<'pass, 'encoder> CallbackRenderPass<'pass, 'encoder> {
    pub(crate) fn new(
        pass: &'pass mut wgpu::RenderPass<'encoder>,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        Self {
            pass,
            target_format,
            sample_count,
        }
    }

    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    pub fn set_pipeline(&mut self, pipeline: &wgpu::RenderPipeline) {
        self.pass.set_pipeline(pipeline);
    }

    pub fn set_bind_group(&mut self, index: u32, bind_group: &wgpu::BindGroup, offsets: &[u32]) {
        self.pass.set_bind_group(index, bind_group, offsets);
    }

    pub fn set_vertex_buffer(&mut self, slot: u32, buffer: wgpu::BufferSlice<'_>) {
        self.pass.set_vertex_buffer(slot, buffer);
    }

    pub fn set_index_buffer(&mut self, buffer: wgpu::BufferSlice<'_>, format: wgpu::IndexFormat) {
        self.pass.set_index_buffer(buffer, format);
    }

    pub fn set_viewport(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    ) {
        self.pass
            .set_viewport(x, y, width, height, min_depth, max_depth);
    }

    pub fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.pass.set_scissor_rect(x, y, width, height);
    }

    pub fn set_stencil_reference(&mut self, reference: u32) {
        self.pass.set_stencil_reference(reference);
    }

    pub fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        self.pass.draw(vertices, instances);
    }

    pub fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    ) {
        self.pass.draw_indexed(indices, base_vertex, instances);
    }
}

impl<'pass, 'encoder> Deref for CallbackRenderPass<'pass, 'encoder> {
    type Target = wgpu::RenderPass<'encoder>;

    fn deref(&self) -> &Self::Target {
        self.pass
    }
}

impl<'pass, 'encoder> DerefMut for CallbackRenderPass<'pass, 'encoder> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.pass
    }
}

/// Trait for custom wgpu rendering inside a `repose` layout rect.
pub trait WgpuCallback: Send + Sync + 'static {
    fn resource_key(&self) -> Option<&str> {
        None
    }

    fn resource_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }

    /// Called before the main `repose` render pass, with access to `device`/`queue`/`encoder`
    /// for buffer uploads. Can return extra command buffers to be submitted.
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _encoder: &mut wgpu::CommandEncoder,
        _screen_descriptor: &ScreenDescriptor,
        _resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    /// Called after all `prepare` calls, before `paint`. For cross-callback sync.
    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _encoder: &mut wgpu::CommandEncoder,
        _screen_descriptor: &ScreenDescriptor,
        _resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint(
        &self,
        info: PaintCallbackInfo,
        render_pass: &mut CallbackRenderPass<'_, '_>,
        resources: &CallbackResources,
    );
}

pub struct Callback(pub Box<dyn WgpuCallback>);

impl Callback {
    #[deprecated(note = "rect is ignored; use Callback::new(callback) - layout supplies rect")]
    pub fn new_paint_callback(
        _rect: Rect,
        callback: impl WgpuCallback + 'static,
    ) -> PaintCallbackPayload {
        Arc::new(Self(Box::new(callback)))
    }

    /// Create payload without caring about rect (rect is supplied via `SceneNode`).
    // Public API used downstream; keep the `new` name despite the Arc return.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(callback: impl WgpuCallback + 'static) -> PaintCallbackPayload {
        Arc::new(Self(Box::new(callback)))
    }

    /// Idiomatic helper: create an `Embedded` view directly from a `WgpuCallback`.
    /// `Modifier` supplies the layout rect.
    pub fn embedded_view(
        modifier: repose_core::Modifier,
        callback: impl WgpuCallback + 'static,
    ) -> repose_core::View {
        let payload = Self::new(callback);
        let mut m = modifier.paint_callback(payload);
        let has_size = m.size.is_some()
            || m.width.is_some()
            || m.height.is_some()
            || m.fill_max.is_some()
            || m.fill_max_w.is_some()
            || m.fill_max_h.is_some();
        if !has_size {
            m = m.size(repose_core::Dp(100.0), repose_core::Dp(100.0));
        }
        repose_core::View::new(0, repose_core::ViewKind::Box).modifier(m)
    }
}
