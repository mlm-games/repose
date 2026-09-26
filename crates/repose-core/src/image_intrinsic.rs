//! Natural (intrinsic) pixel size of uploaded images.
//!
//! Layout needs an image's natural size so an unsized [`Image`](crate::view::ViewKind::Image)
//! view reports the same intrinsic size that Compose's `PainterNode.modifyConstraints`
//! and Godot's `TextureRect::get_minimum_size` produce. Sizes are recorded here
//! as the image is created rather than decoded on demand, so layout never
//! depends on a renderer being present.
//!
//! Raw uploads (`set_image_rgba8`, `set_image_planes`, `set_image_dmabuf`) know
//! their dimensions at the call site and register immediately. Encoded uploads
//! are decoded by the renderer, which publishes the size once it has them.

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::ImageHandle;

fn registry() -> &'static RwLock<HashMap<ImageHandle, (u32, u32)>> {
    static REGISTRY: OnceLock<RwLock<HashMap<ImageHandle, (u32, u32)>>> = OnceLock::new();
    REGISTRY.get_or_init(RwLock::default)
}

/// Record `handle`'s natural pixel size and schedule a frame so layout can pick
/// it up. A zero dimension means the image has no usable intrinsic size and is
/// ignored, leaving the view to fall back to `None`.
pub fn set_image_intrinsic_size(handle: ImageHandle, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    let changed = registry().write().insert(handle, (width, height));
    if changed != Some((width, height)) {
        crate::request_frame();
    }
}

/// Natural pixel size of `handle`, or `None` while it is unknown or removed.
pub fn image_intrinsic_size(handle: ImageHandle) -> Option<(u32, u32)> {
    registry().read().get(&handle).copied()
}

/// Forget `handle`'s recorded size. Called when the image is removed.
pub fn clear_image_intrinsic_size(handle: ImageHandle) {
    if registry().write().remove(&handle).is_some() {
        crate::request_frame();
    }
}
