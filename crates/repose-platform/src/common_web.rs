use winit::window::{ImePurpose, Window};

pub(crate) fn set_ime_for_textfield(window: &Window, is_textfield: bool) {
    if is_textfield {
        window.set_ime_allowed(true);
        window.set_ime_purpose(ImePurpose::Normal);
    } else {
        window.set_ime_allowed(false);
    }
}
