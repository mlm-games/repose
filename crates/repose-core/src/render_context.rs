use std::sync::Arc;

use crate::color::{ColorInfo, PixelFormat};
use crate::{ImageHandle, request_present};

#[derive(Debug)]
pub enum RenderCommand {
    SetImageEncoded {
        handle: ImageHandle,
        bytes: Vec<u8>,
        srgb: bool,
    },
    SetImageRgba8 {
        handle: ImageHandle,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
        srgb: bool,
    },
    SetImageNv12 {
        handle: ImageHandle,
        w: u32,
        h: u32,
        y: Vec<u8>,
        uv: Vec<u8>,
        color_info: ColorInfo,
    },
    SetImagePlanes {
        handle: ImageHandle,
        w: u32,
        h: u32,
        pixel_format: PixelFormat,
        planes: Vec<Arc<[u8]>>,
        color_info: ColorInfo,
    },
    #[cfg(target_os = "linux")]
    SetImageDmaBuf {
        handle: ImageHandle,
        w: u32,
        h: u32,
        fds: Vec<std::os::unix::io::OwnedFd>,
        fourcc: u32,
        modifier: u64,
        strides: Vec<u32>,
        offsets: Vec<u64>,
        color_info: ColorInfo,
    },
    RemoveImage {
        handle: ImageHandle,
    },
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    struct Queue {
        updates: HashMap<ImageHandle, RenderCommand>,
        removals: HashSet<ImageHandle>,
    }

    impl Queue {
        fn new() -> Self {
            Self {
                updates: HashMap::new(),
                removals: HashSet::new(),
            }
        }
    }

    #[derive(Clone)]
    pub struct RenderContext {
        next: Arc<AtomicU64>,
        q: Arc<Mutex<Queue>>,
    }

    impl RenderContext {
        pub fn new() -> Self {
            Self {
                next: Arc::new(AtomicU64::new(1)),
                q: Arc::new(Mutex::new(Queue::new())),
            }
        }

        pub fn alloc_image_handle(&self) -> ImageHandle {
            self.next.fetch_add(1, Ordering::Relaxed)
        }

        pub fn set_image_encoded(&self, handle: ImageHandle, bytes: Vec<u8>, srgb: bool) {
            let mut q = self.q.lock().unwrap();
            q.removals.remove(&handle);
            q.updates.insert(
                handle,
                RenderCommand::SetImageEncoded {
                    handle,
                    bytes,
                    srgb,
                },
            );
            request_present();
        }

        pub fn image_from_encoded(
            &self,
            bytes: impl Into<Vec<u8>>,
            srgb: bool,
        ) -> ImageHandle {
            let handle = self.alloc_image_handle();
            self.set_image_encoded(handle, bytes.into(), srgb);
            handle
        }

        pub fn set_image_rgba8(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            rgba: Vec<u8>,
            srgb: bool,
        ) {
            let mut q = self.q.lock().unwrap();
            q.removals.remove(&handle);
            q.updates.insert(
                handle,
                RenderCommand::SetImageRgba8 {
                    handle,
                    w,
                    h,
                    rgba,
                    srgb,
                },
            );
            request_present();
        }

        pub fn set_image_nv12(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            y: Arc<[u8]>,
            uv: Arc<[u8]>,
            color_info: ColorInfo,
        ) {
            self.set_image_planes(handle, w, h, PixelFormat::Nv12, vec![y, uv], color_info);
        }

        pub fn set_image_planes(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            pixel_format: PixelFormat,
            planes: Vec<Arc<[u8]>>,
            color_info: ColorInfo,
        ) {
            let mut q = self.q.lock().unwrap();
            q.removals.remove(&handle);
            q.updates.insert(
                handle,
                RenderCommand::SetImagePlanes {
                    handle,
                    w,
                    h,
                    pixel_format,
                    planes,
                    color_info,
                },
            );
            request_present();
        }

        pub fn remove_image(&self, handle: ImageHandle) {
            let mut q = self.q.lock().unwrap();
            q.removals.insert(handle);
            q.updates.remove(&handle);
            request_present();
        }

        #[cfg(target_os = "linux")]
        pub fn set_image_dmabuf(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            fds: Vec<std::os::unix::io::OwnedFd>,
            fourcc: u32,
            modifier: u64,
            strides: Vec<u32>,
            offsets: Vec<u64>,
            color_info: ColorInfo,
        ) {
            let mut q = self.q.lock().unwrap();
            q.removals.remove(&handle);
            q.updates.insert(
                handle,
                RenderCommand::SetImageDmaBuf {
                    handle,
                    w,
                    h,
                    fds,
                    fourcc,
                    modifier,
                    strides,
                    offsets,
                    color_info,
                },
            );
            request_present();
        }

        pub fn drain(&self) -> Vec<RenderCommand> {
            let mut q = self.q.lock().unwrap();
            let mut result = Vec::with_capacity(q.removals.len() + q.updates.len());

            for handle in q.removals.drain() {
                result.push(RenderCommand::RemoveImage { handle });
            }

            for (_, cmd) in q.updates.drain() {
                result.push(cmd);
            }

            result
        }
    }

    impl Default for RenderContext {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;
    use std::sync::Arc;

    struct Queue {
        updates: HashMap<ImageHandle, RenderCommand>,
        removals: HashSet<ImageHandle>,
    }

    struct Inner {
        next: ImageHandle,
        q: Queue,
    }

    #[derive(Clone)]
    pub struct RenderContext {
        inner: Rc<RefCell<Inner>>,
    }

    impl RenderContext {
        pub fn new() -> Self {
            Self {
                inner: Rc::new(RefCell::new(Inner {
                    next: 1,
                    q: Queue {
                        updates: HashMap::new(),
                        removals: HashSet::new(),
                    },
                })),
            }
        }

        pub fn alloc_image_handle(&self) -> ImageHandle {
            let mut s = self.inner.borrow_mut();
            let id = s.next;
            s.next += 1;
            id
        }

        pub fn set_image_encoded(&self, handle: ImageHandle, bytes: Vec<u8>, srgb: bool) {
            let mut s = self.inner.borrow_mut();
            s.q.removals.remove(&handle);
            s.q.updates.insert(
                handle,
                RenderCommand::SetImageEncoded {
                    handle,
                    bytes,
                    srgb,
                },
            );
            request_present();
        }

        pub fn image_from_encoded(
            &self,
            bytes: impl Into<Vec<u8>>,
            srgb: bool,
        ) -> ImageHandle {
            let handle = self.alloc_image_handle();
            self.set_image_encoded(handle, bytes.into(), srgb);
            handle
        }

        pub fn set_image_rgba8(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            rgba: Vec<u8>,
            srgb: bool,
        ) {
            let mut s = self.inner.borrow_mut();
            s.q.removals.remove(&handle);
            s.q.updates.insert(
                handle,
                RenderCommand::SetImageRgba8 {
                    handle,
                    w,
                    h,
                    rgba,
                    srgb,
                },
            );
            request_present();
        }

        pub fn set_image_nv12(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            y: Arc<[u8]>,
            uv: Arc<[u8]>,
            color_info: ColorInfo,
        ) {
            self.set_image_planes(handle, w, h, PixelFormat::Nv12, vec![y, uv], color_info);
        }

        pub fn set_image_planes(
            &self,
            handle: ImageHandle,
            w: u32,
            h: u32,
            pixel_format: PixelFormat,
            planes: Vec<Arc<[u8]>>,
            color_info: ColorInfo,
        ) {
            let mut s = self.inner.borrow_mut();
            s.q.removals.remove(&handle);
            s.q.updates.insert(
                handle,
                RenderCommand::SetImagePlanes {
                    handle,
                    w,
                    h,
                    pixel_format,
                    planes,
                    color_info,
                },
            );
            request_present();
        }

        pub fn remove_image(&self, handle: ImageHandle) {
            let mut s = self.inner.borrow_mut();
            s.q.updates.remove(&handle);
            s.q.removals.insert(handle);
            request_present();
        }

        #[cfg(target_os = "linux")]
        pub fn set_image_dmabuf(
            &self,
            _handle: ImageHandle,
            _w: u32,
            _h: u32,
            _fds: Vec<std::os::unix::io::OwnedFd>,
            _fourcc: u32,
            _modifier: u64,
            _strides: Vec<u32>,
            _offsets: Vec<u64>,
            _color_info: ColorInfo,
        ) {
            // DMA-BUF not supported on WASM
        }

        pub fn drain(&self) -> Vec<RenderCommand> {
            let mut s = self.inner.borrow_mut();
            let mut result = Vec::with_capacity(s.q.removals.len() + s.q.updates.len());

            for handle in s.q.removals.drain() {
                result.push(RenderCommand::RemoveImage { handle });
            }

            for (_, cmd) in s.q.updates.drain() {
                result.push(cmd);
            }

            result
        }
    }

    impl Default for RenderContext {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use imp::RenderContext;

/// RAII guard that removes the image when dropped
pub struct ImageHandleGuard {
    pub handle: ImageHandle,
    rc: RenderContext,
}

impl ImageHandleGuard {
    pub fn new(rc: &RenderContext) -> Self {
        Self {
            handle: rc.alloc_image_handle(),
            rc: rc.clone(),
        }
    }
}

impl Drop for ImageHandleGuard {
    fn drop(&mut self) {
        self.rc.remove_image(self.handle);
    }
}

impl std::ops::Deref for ImageHandleGuard {
    type Target = ImageHandle;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}
