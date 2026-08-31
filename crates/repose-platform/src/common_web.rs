use repose_core::{ImePurposeHint, KeyboardCapitalization};
use winit::window::{ImePurpose, Window};

/// winit only distinguishes Normal / Password / Terminal, so the remaining
/// hints fall back to Normal; richer hints (email/url/phone/number) are meant
/// for platforms that can react to them (e.g. the web `inputmode` attribute,
/// once a mirror-input manager is wired on wasm).
pub(crate) fn map_ime_purpose(hint: ImePurposeHint) -> ImePurpose {
    match hint {
        ImePurposeHint::Password => ImePurpose::Password,
        ImePurposeHint::Normal
        | ImePurposeHint::Email
        | ImePurposeHint::Url
        | ImePurposeHint::Phone
        | ImePurposeHint::Number => ImePurpose::Normal,
    }
}

/// Enable/disable IME for the given window with default keyboard hints.
pub(crate) fn set_ime_for_textfield(window: &Window, is_textfield: bool) {
    set_ime_for_textfield_ex(
        window,
        is_textfield,
        ImePurposeHint::Normal,
        true,
        KeyboardCapitalization::Unspecified,
    );
}

/// Where winit supports it (desktop X11/Wayland/Windows) this drives the OS
/// IME purpose. The capitalization/auto-correct hints are informational for
/// platforms that can react to them.
pub(crate) fn set_ime_for_textfield_ex(
    window: &Window,
    is_textfield: bool,
    purpose: ImePurposeHint,
    _auto_correct: bool,
    _capitalization: KeyboardCapitalization,
) {
    if is_textfield {
        window.set_ime_allowed(true);
        window.set_ime_purpose(map_ime_purpose(purpose));
    } else {
        window.set_ime_allowed(false);
    }
}
