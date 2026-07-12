use repose_ui::TextFieldState;
use winit::event::Ime;

pub(crate) fn handle_ime_event(
    ime: Ime,
    state: &mut TextFieldState,
    notify_text_change: &mut impl FnMut(String),
    ime_preedit: &mut bool,
) {
    match ime {
        Ime::Enabled => {
            *ime_preedit = false;
        }
        Ime::Preedit(text, cursor) => {
            let cursor_usize = cursor.map(|(a, b)| (a, b));
            state.set_composition(text.clone(), cursor_usize);
            *ime_preedit = !text.is_empty();
            repose_ui::textfield::ensure_caret_visible(state, true);
            notify_text_change(state.text.clone());
        }
        Ime::Commit(text) => {
            state.commit_composition(text);
            *ime_preedit = false;
            repose_ui::textfield::ensure_caret_visible(state, true);
            notify_text_change(state.text.clone());
        }
        Ime::Disabled => {
            *ime_preedit = false;
            if state.composition.is_some() {
                state.cancel_composition();
                repose_ui::textfield::ensure_caret_visible(state, true);
                notify_text_change(state.text.clone());
            }
        }
    }
}
