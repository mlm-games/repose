//! Unified offscreen helper (native/android/wasm).

use anyhow::Result;
use wgpu::{PollType, TextureFormat};

use crate::WgpuSceneRenderer;
use repose_core::Scene;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
static SHARED_DEVICE: OnceLock<Mutex<Option<(wgpu::Device, wgpu::Queue)>>> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
fn shared_slot() -> &'static Mutex<Option<(wgpu::Device, wgpu::Queue)>> {
    SHARED_DEVICE.get_or_init(|| Mutex::new(None))
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SHARED_DEVICE_WASM: std::cell::RefCell<Option<(wgpu::Device, wgpu::Queue)>> = std::cell::RefCell::new(None);
}

/// Publish live Device/Queue for shared offscreen.
pub fn set_shared_device(device: wgpu::Device, queue: wgpu::Queue) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(mut g) = shared_slot().lock() {
            *g = Some((device, queue));
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        SHARED_DEVICE_WASM.with(|c| *c.borrow_mut() = Some((device, queue)));
    }
}
pub fn shared_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        shared_slot().lock().ok().and_then(|g| g.clone())
    }
    #[cfg(target_arch = "wasm32")]
    {
        SHARED_DEVICE_WASM.with(|c| c.borrow().clone())
    }
}
pub fn clear_shared_device() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(mut g) = shared_slot().lock() {
            *g = None;
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        SHARED_DEVICE_WASM.with(|c| *c.borrow_mut() = None);
    }
}

pub struct OffscreenRenderer {
    renderer: WgpuSceneRenderer,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
}

impl OffscreenRenderer {
    pub async fn new(width: u32, height: u32, msaa: u32) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        let instance = if cfg!(target_arch = "wasm32") {
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            wgpu::util::new_instance_with_webgpu_detection(desc).await
        } else {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle())
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                ..Default::default()
            })
            .await?;
        let format = TextureFormat::Rgba8UnormSrgb;
        let msaa = crate::pick_surface_msaa(&adapter, format, msaa);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("repose-offscreen"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                experimental_features: wgpu::ExperimentalFeatures::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;
        let renderer = WgpuSceneRenderer::from_device(device, queue, format, msaa);
        Self::from_renderer(renderer, width, height)
    }

    pub fn new_blocking(width: u32, height: u32, msaa: u32) -> Result<Self> {
        pollster::block_on(Self::new(width, height, msaa))
    }

    /// Shared-device: reuse Device/Queue, no Adapter.
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        msaa: u32,
    ) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        let format = TextureFormat::Rgba8UnormSrgb;
        let renderer = WgpuSceneRenderer::from_device(device, queue, format, msaa.max(1));
        Self::from_renderer(renderer, width, height)
    }

    pub fn from_device_with_adapter(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
        msaa: u32,
    ) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        let format = TextureFormat::Rgba8UnormSrgb;
        let msaa = crate::pick_surface_msaa(adapter, format, msaa);
        let renderer = WgpuSceneRenderer::from_device(device, queue, format, msaa);
        Self::from_renderer(renderer, width, height)
    }

    pub fn from_renderer(mut renderer: WgpuSceneRenderer, width: u32, height: u32) -> Result<Self> {
        renderer.resize(width, height);
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("repose-offscreen-tex"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(align) * align;
        let buf_size = (padded * height) as u64;
        let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("repose-offscreen-readback"),
            size: buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            renderer,
            texture,
            view,
            readback,
            width,
            height,
            padded_bytes_per_row: padded,
        })
    }

    fn encode_rgba(&mut self, scene: &Scene, clear: Option<[f64; 4]>) -> wgpu::CommandBuffer {
        let mut encoder =
            self.renderer
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("repose-offscreen-encoder"),
                });
        self.renderer
            .render_scene_to_encoder(scene, &mut encoder, &self.view, clear);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        encoder.finish()
    }

    pub fn render_rgba(&mut self, scene: &Scene, clear: Option<[f64; 4]>) -> Result<Vec<u8>> {
        let cmd = self.encode_rgba(scene, clear);
        self.renderer.queue.submit(Some(cmd));
        let slice = self.readback.slice(..);
        let (tx, rx) = web_workers::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send_sync(r);
        });
        self.renderer.device.poll(PollType::wait_indefinitely())?;
        rx.recv_sync()??;
        let mapped = slice.get_mapped_range()?;
        let out = strip_padding(&mapped, self.width, self.height, self.padded_bytes_per_row);
        drop(mapped);
        self.readback.unmap();
        Ok(out)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn render_rgba_async(
        &mut self,
        scene: &Scene,
        clear: Option<[f64; 4]>,
    ) -> Result<Vec<u8>> {
        let cmd = self.encode_rgba(scene, clear);
        self.renderer.queue.submit(Some(cmd));
        let slice = self.readback.slice(..);
        let (tx, rx) = web_workers::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, {
            let tx = tx.clone();
            move |r| {
                let _ = tx.send_sync(r);
            }
        });
        loop {
            let _ = self.renderer.device.poll(PollType::Poll);
            if let Ok(r) = rx.try_recv() {
                r?;
                break;
            }
            web_workers::web::yield_now_async(web_workers::web::YieldTime::UserVisible).await;
        }
        let mapped = slice.get_mapped_range()?;
        let out = strip_padding(&*mapped, self.width, self.height, self.padded_bytes_per_row);
        drop(mapped);
        self.readback.unmap();
        Ok(out)
    }

    pub fn renderer_mut(&mut self) -> &mut WgpuSceneRenderer {
        &mut self.renderer
    }
    pub fn renderer(&self) -> &WgpuSceneRenderer {
        &self.renderer
    }

    pub fn ensure_size(&mut self, width: u32, height: u32) -> Result<()> {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return Ok(());
        }
        self.renderer.resize(width, height);
        let texture = self
            .renderer
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("repose-offscreen-tex"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = (width * 4).div_ceil(align) * align;
        let readback = self.renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("repose-offscreen-readback"),
            size: (padded * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.texture = texture;
        self.view = view;
        self.readback = readback;
        self.width = width;
        self.height = height;
        self.padded_bytes_per_row = padded;
        Ok(())
    }

    pub async fn render_rgba_unified(
        &mut self,
        scene: &Scene,
        clear: Option<[f64; 4]>,
    ) -> Result<Vec<u8>> {
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        {
            if web_workers::web::has_block_support() {
                return self.render_rgba(scene, clear);
            } else {
                return self.render_rgba_async(scene, clear).await;
            }
        }
        #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
        {
            self.render_rgba(scene, clear)
        }
    }
}

fn strip_padding(mapped: &[u8], width: u32, height: u32, padded: u32) -> Vec<u8> {
    let unpadded = width * 4;
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let start = (y * padded) as usize;
        let end = start + unpadded as usize;
        out.extend_from_slice(&mapped[start..end]);
    }
    out
}

pub fn map_buffer_blocking(slice: &wgpu::BufferSlice<'_>, device: &wgpu::Device) -> Result<()> {
    let (tx, rx) = web_workers::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send_sync(r);
    });
    device.poll(PollType::wait_indefinitely())?;
    rx.recv_sync()??;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn map_buffer_async(slice: &wgpu::BufferSlice<'_>, device: &wgpu::Device) -> Result<()> {
    let (tx, rx) = web_workers::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, {
        let tx = tx.clone();
        move |r| {
            let _ = tx.send_sync(r);
        }
    });
    loop {
        let _ = device.poll(PollType::Poll);
        if let Ok(r) = rx.try_recv() {
            r?;
            break;
        }
        web_workers::web::yield_now_async(web_workers::web::YieldTime::UserVisible).await;
    }
    Ok(())
}

pub async fn map_buffer_unified(
    slice: &wgpu::BufferSlice<'_>,
    device: &wgpu::Device,
) -> Result<()> {
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        if web_workers::web::has_block_support() {
            return map_buffer_blocking(slice, device);
        } else {
            return map_buffer_async(slice, device).await;
        }
    }
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        map_buffer_blocking(slice, device)
    }
}
