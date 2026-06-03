use repose_core::Rect;
use repose_core::locals::dp_to_px;
use repose_ui::TextFieldState;
use repose_ui::textfield::{TF_FONT_DP, TF_PADDING_X_DP, measure_text};
use winit::event::Ime;

pub(crate) fn handle_ime_event(
    ime: Ime,
    state: &mut TextFieldState,
    hit_rect: Rect,
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
            ensure_caret_visible_in_hit(state, hit_rect);
            notify_text_change(state.text.clone());
        }
        Ime::Commit(text) => {
            state.commit_composition(text);
            *ime_preedit = false;
            ensure_caret_visible_in_hit(state, hit_rect);
            notify_text_change(state.text.clone());
        }
        Ime::Disabled => {
            *ime_preedit = false;
            if state.composition.is_some() {
                state.cancel_composition();
                ensure_caret_visible_in_hit(state, hit_rect);
                notify_text_change(state.text.clone());
            }
        }
    }
}

fn ensure_caret_visible_in_hit(state: &mut TextFieldState, hit_rect: Rect) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let m = measure_text(&state.text, font_px, None);
    let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
    state.ensure_caret_visible(
        caret_x_px,
        hit_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
        dp_to_px(2.0),
    );
}
