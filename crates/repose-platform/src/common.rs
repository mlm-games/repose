use crate::*;

pub(crate) fn request_redraw(window: &Option<std::sync::Arc<winit::window::Window>>) {
    if let Some(w) = window {
        w.request_redraw();
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn is_textfield_in_frame(frame_cache: &Option<Frame>, id: u64) -> bool {
    if let Some(f) = frame_cache {
        f.semantics_nodes
            .iter()
            .any(|n| n.id == id && n.role == Role::TextField)
    } else {
        false
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn update_modifiers(modifiers: &mut Modifiers, state: &winit::keyboard::ModifiersState) {
    modifiers.shift = state.shift_key();
    modifiers.ctrl = state.control_key();
    modifiers.alt = state.alt_key();
    modifiers.meta = state.super_key();
    modifiers.command = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
}

pub(crate) fn map_key(key: winit::keyboard::PhysicalKey) -> repose_core::input::Key {
    use repose_core::input::Key;
    use winit::keyboard::{KeyCode, PhysicalKey};

    match key {
        PhysicalKey::Code(KeyCode::Enter) => Key::Enter,
        PhysicalKey::Code(KeyCode::Tab) => Key::Tab,
        PhysicalKey::Code(KeyCode::Backspace) => Key::Backspace,
        PhysicalKey::Code(KeyCode::Delete) => Key::Delete,
        PhysicalKey::Code(KeyCode::Escape) => Key::Escape,
        PhysicalKey::Code(KeyCode::ArrowLeft) => Key::ArrowLeft,
        PhysicalKey::Code(KeyCode::ArrowRight) => Key::ArrowRight,
        PhysicalKey::Code(KeyCode::ArrowUp) => Key::ArrowUp,
        PhysicalKey::Code(KeyCode::ArrowDown) => Key::ArrowDown,
        PhysicalKey::Code(KeyCode::Home) => Key::Home,
        PhysicalKey::Code(KeyCode::End) => Key::End,
        PhysicalKey::Code(KeyCode::PageUp) => Key::PageUp,
        PhysicalKey::Code(KeyCode::PageDown) => Key::PageDown,
        PhysicalKey::Code(KeyCode::Space) => Key::Space,
        PhysicalKey::Code(KeyCode::KeyA) => Key::Character('a'),
        PhysicalKey::Code(KeyCode::KeyB) => Key::Character('b'),
        PhysicalKey::Code(KeyCode::KeyC) => Key::Character('c'),
        PhysicalKey::Code(KeyCode::KeyD) => Key::Character('d'),
        PhysicalKey::Code(KeyCode::KeyE) => Key::Character('e'),
        PhysicalKey::Code(KeyCode::KeyF) => Key::Character('f'),
        PhysicalKey::Code(KeyCode::KeyG) => Key::Character('g'),
        PhysicalKey::Code(KeyCode::KeyH) => Key::Character('h'),
        PhysicalKey::Code(KeyCode::KeyI) => Key::Character('i'),
        PhysicalKey::Code(KeyCode::KeyJ) => Key::Character('j'),
        PhysicalKey::Code(KeyCode::KeyK) => Key::Character('k'),
        PhysicalKey::Code(KeyCode::KeyL) => Key::Character('l'),
        PhysicalKey::Code(KeyCode::KeyM) => Key::Character('m'),
        PhysicalKey::Code(KeyCode::KeyN) => Key::Character('n'),
        PhysicalKey::Code(KeyCode::KeyO) => Key::Character('o'),
        PhysicalKey::Code(KeyCode::KeyP) => Key::Character('p'),
        PhysicalKey::Code(KeyCode::KeyQ) => Key::Character('q'),
        PhysicalKey::Code(KeyCode::KeyR) => Key::Character('r'),
        PhysicalKey::Code(KeyCode::KeyS) => Key::Character('s'),
        PhysicalKey::Code(KeyCode::KeyT) => Key::Character('t'),
        PhysicalKey::Code(KeyCode::KeyU) => Key::Character('u'),
        PhysicalKey::Code(KeyCode::KeyV) => Key::Character('v'),
        PhysicalKey::Code(KeyCode::KeyW) => Key::Character('w'),
        PhysicalKey::Code(KeyCode::KeyX) => Key::Character('x'),
        PhysicalKey::Code(KeyCode::KeyY) => Key::Character('y'),
        PhysicalKey::Code(KeyCode::KeyZ) => Key::Character('z'),
        PhysicalKey::Code(KeyCode::Digit0) => Key::Character('0'),
        PhysicalKey::Code(KeyCode::Digit1) => Key::Character('1'),
        PhysicalKey::Code(KeyCode::Digit2) => Key::Character('2'),
        PhysicalKey::Code(KeyCode::Digit3) => Key::Character('3'),
        PhysicalKey::Code(KeyCode::Digit4) => Key::Character('4'),
        PhysicalKey::Code(KeyCode::Digit5) => Key::Character('5'),
        PhysicalKey::Code(KeyCode::Digit6) => Key::Character('6'),
        PhysicalKey::Code(KeyCode::Digit7) => Key::Character('7'),
        PhysicalKey::Code(KeyCode::Digit8) => Key::Character('8'),
        PhysicalKey::Code(KeyCode::Digit9) => Key::Character('9'),
        PhysicalKey::Code(KeyCode::F1) => Key::F(1),
        PhysicalKey::Code(KeyCode::F2) => Key::F(2),
        PhysicalKey::Code(KeyCode::F3) => Key::F(3),
        PhysicalKey::Code(KeyCode::F4) => Key::F(4),
        PhysicalKey::Code(KeyCode::F5) => Key::F(5),
        PhysicalKey::Code(KeyCode::F6) => Key::F(6),
        PhysicalKey::Code(KeyCode::F7) => Key::F(7),
        PhysicalKey::Code(KeyCode::F8) => Key::F(8),
        PhysicalKey::Code(KeyCode::F9) => Key::F(9),
        PhysicalKey::Code(KeyCode::F10) => Key::F(10),
        PhysicalKey::Code(KeyCode::F11) => Key::F(11),
        PhysicalKey::Code(KeyCode::F12) => Key::F(12),
        _ => Key::Unknown,
    }
}

pub(crate) fn process_render_commands(
    backend: &mut repose_render_wgpu::WgpuBackend,
    cmds: Vec<RenderCommand>,
) {
    for cmd in cmds {
        match cmd {
            RenderCommand::SetImageEncoded {
                handle,
                bytes,
                srgb,
            } => {
                let _ = backend.set_image_from_bytes(handle, &bytes, srgb);
            }
            RenderCommand::SetImageRgba8 {
                handle,
                w,
                h,
                rgba,
                srgb,
            } => {
                let _ = backend.set_image_rgba8(handle, w, h, &rgba, srgb);
            }
            RenderCommand::SetImageNv12 {
                handle,
                w,
                h,
                y,
                uv,
                color_info,
            } => {
                let _ = backend.set_image_nv12(handle, w, h, &y, &uv, color_info);
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
                let _ = backend.set_image_planes(handle, w, h, pixel_format, &refs, color_info);
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
                if let Err(e) = backend
                    .set_image_dmabuf(handle, w, h, fds, modifier, strides, offsets, color_info)
                {
                    log::warn!("set_image_dmabuf failed: {e:?}");
                }
            }
            RenderCommand::RemoveImage { handle } => {
                backend.remove_image(handle);
            }
        }
    }
}
