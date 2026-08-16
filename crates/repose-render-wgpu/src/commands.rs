//! Shared application of render commands queued by `RenderContext`.
//!
//! Platform runners (desktop/android/web) and embedders (bevy, baseview)
//! can move image set/remove commands from the reactive scene into the GPU
//! texture cache through this.

use repose_core::RenderCommand;

use crate::WgpuSceneRenderer;

/// Apply a batch of [`RenderCommand`]s to a scene renderer.
///
/// `WgpuSurfaceBackend` derefs to `WgpuSceneRenderer`, so this works for
/// both the windowed backends and offscreen-device embedders.
pub fn apply_render_commands(renderer: &mut WgpuSceneRenderer, cmds: Vec<RenderCommand>) {
    for cmd in cmds {
        match cmd {
            RenderCommand::SetImageEncoded {
                handle,
                bytes,
                srgb,
            } => {
                if let Err(e) = renderer.set_image_from_bytes(handle, &bytes, srgb) {
                    log::warn!("repose-render: SetImageEncoded({handle}): {e:#}");
                }
            }
            RenderCommand::SetImageRgba8 {
                handle,
                w,
                h,
                rgba,
                srgb,
            } => {
                if let Err(e) = renderer.set_image_rgba8(handle, w, h, &rgba, srgb) {
                    log::warn!("repose-render: SetImageRgba8({handle}): {e:#}");
                }
            }
            RenderCommand::SetImageNv12 {
                handle,
                w,
                h,
                y,
                uv,
                color_info,
            } => {
                if let Err(e) = renderer.set_image_nv12(handle, w, h, &y, &uv, color_info) {
                    log::warn!("repose-render: SetImageNv12({handle}): {e:#}");
                }
            }
            RenderCommand::SetImagePlanes {
                handle,
                w,
                h,
                pixel_format,
                planes,
                color_info,
            } => {
                let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_ref()).collect();
                if let Err(e) =
                    renderer.set_image_planes(handle, w, h, pixel_format, &refs, color_info)
                {
                    log::warn!("repose-render: SetImagePlanes({handle}): {e:#}");
                }
            }
            #[cfg(target_os = "linux")]
            RenderCommand::SetImageDmaBuf {
                handle,
                w,
                h,
                fds,
                fourcc: _,
                modifier,
                strides,
                offsets,
                color_info,
            } => {
                if let Err(e) = renderer
                    .set_image_dmabuf(handle, w, h, fds, modifier, strides, offsets, color_info)
                {
                    log::warn!("repose-render: SetImageDmabuf({handle}): {e:#}");
                }
            }
            RenderCommand::RemoveImage { handle } => {
                renderer.remove_image(handle);
            }
        }
    }
}
