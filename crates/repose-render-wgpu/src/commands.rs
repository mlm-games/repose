//! Shared application of render commands queued by `RenderContext`.
//!
//! Platform runners (desktop/android/web) and embedders (bevy, baseview)
//! can move image set/remove commands from the reactive scene into the GPU
//! texture cache through this.

use repose_core::RenderCommand;

use crate::WgpuSceneRenderer;

/// A command that could not be applied. The command is consumed either way
/// and is never retried, so callers that care about a specific handle get
/// the handle back here.
#[derive(Clone, Debug)]
pub struct RenderCommandFailure {
    pub kind: &'static str,
    pub handle: u64,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct RenderCommandReport {
    pub applied: usize,
    pub failures: Vec<RenderCommandFailure>,
}

impl RenderCommandReport {
    /// Log every failure. Callers sit inside callbacks with no error
    /// channel, so this is the usual way the report reaches a human.
    pub fn log_failures(&self) {
        for failure in &self.failures {
            log::warn!(
                "repose-render: {}({}) failed: {}",
                failure.kind,
                failure.handle,
                failure.error
            );
        }
    }
}

/// Apply a batch of [`RenderCommand`]s to a scene renderer.
///
/// `WgpuSurfaceBackend` derefs to `WgpuSceneRenderer`, so this works for
/// both the windowed backends and offscreen-device embedders. Failures are
/// collected rather than logged here: every caller sits inside a callback
/// with no error channel (`winit` event loop, `requestAnimationFrame`),
/// so reporting is the caller's job.
pub fn apply_render_commands(
    renderer: &mut WgpuSceneRenderer,
    cmds: Vec<RenderCommand>,
) -> RenderCommandReport {
    let mut report = RenderCommandReport::default();
    for cmd in cmds {
        let (kind, handle, result) = match cmd {
            RenderCommand::SetImageEncoded {
                handle,
                bytes,
                srgb,
            } => (
                "SetImageEncoded",
                handle,
                renderer.set_image_from_bytes(handle, &bytes, srgb),
            ),
            RenderCommand::SetImageRgba8 {
                handle,
                w,
                h,
                rgba,
                srgb,
            } => (
                "SetImageRgba8",
                handle,
                renderer.set_image_rgba8(handle, w, h, &rgba, srgb),
            ),
            RenderCommand::SetImageNv12 {
                handle,
                w,
                h,
                y,
                uv,
                color_info,
            } => (
                "SetImageNv12",
                handle,
                renderer.set_image_nv12(handle, w, h, &y, &uv, color_info),
            ),
            RenderCommand::SetImagePlanes {
                handle,
                w,
                h,
                pixel_format,
                planes,
                color_info,
            } => {
                let refs: Vec<&[u8]> = planes.iter().map(|p| p.as_ref()).collect();
                (
                    "SetImagePlanes",
                    handle,
                    renderer.set_image_planes(handle, w, h, pixel_format, &refs, color_info),
                )
            }
            #[cfg(target_os = "linux")]
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
            } => (
                "SetImageDmaBuf",
                handle,
                renderer.set_image_dmabuf_fourcc(
                    handle, w, h, fds, fourcc, modifier, strides, offsets, color_info,
                ),
            ),
            RenderCommand::RemoveImage { handle } => {
                renderer.remove_image(handle);
                ("RemoveImage", handle, Ok(()))
            }
        };
        match result {
            Ok(()) => report.applied += 1,
            Err(error) => report.failures.push(RenderCommandFailure {
                kind,
                handle,
                error: format!("{error:#}"),
            }),
        }
    }
    report
}
