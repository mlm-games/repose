use repose_core::prelude::*;
use repose_ui::pager::{HorizontalPager, PagerState};
use repose_ui::*;
use std::rc::Rc;

pub fn screen() -> View {
    let state: Rc<PagerState> = remember_with_key("pager_state", || PagerState::new(6));

    let page = state.current_page();
    let colors = [
        Color::from_rgb(0xBB, 0x86, 0xFC),
        Color::from_rgb(0x67, 0xBA, 0x80),
        Color::from_rgb(0x5B, 0xBE, 0xE6),
        Color::from_rgb(0xF2, 0xA9, 0x00),
        Color::from_rgb(0xEA, 0x6A, 0x6A),
        Color::from_rgb(0xCF, 0x93, 0xD9),
    ];

    Column(Modifier::new().fill_max_size()).child((
        // Page indicator
        Row(Modifier::new().align_items(AlignItems::Center).padding(16.0)).child(
            (0..6)
                .map(|i| {
                    Box(Modifier::new()
                        .size(8.0, 8.0)
                        .margin(4.0)
                        .background(if i == page { colors[i] } else { theme().outline_variant })
                        .clip_rounded(4.0))
                })
                .collect::<Vec<_>>(),
        ),
        // Pager
        HorizontalPager(
            "demo",
            state.clone(),
            Modifier::new().fill_max_size().margin(16.0),
            move |p| {
                let color = colors[p % colors.len()];
                Surface(
                    Modifier::new()
                        .fill_max_size()
                        .background(color.with_alpha(48))
                        .clip_rounded(24.0),
                    Column(
                        Modifier::new()
                            .fill_max_size()
                            .justify_content(JustifyContent::Center)
                            .align_items(AlignItems::Center),
                    )
                    .child((
                        Text(format!("Page {}", p + 1))
                            .size(48.0)
                            .color(color),
                        Text(format!("Swipe left/right to navigate"))
                            .size(16.0)
                            .color(theme().on_surface.with_alpha(180)),
                    )),
                )
            },
        ),
        // Page controls
        Spacer(),
        Row(Modifier::new().align_items(AlignItems::Center).padding(16.0)).child((
            Button(Text("Prev"), {
                let st = state.clone();
                move || {
                    let p = st.current_page().saturating_sub(1);
                    st.set_page(p);
                }
            }),
            Spacer(),
            Text(format!("Page {} of 6", page + 1))
                .size(16.0)
                .color(theme().on_surface),
            Spacer(),
            Button(Text("Next"), {
                let st = state.clone();
                move || {
                    let p = (st.current_page() + 1).min(5);
                    st.set_page(p);
                }
            }),
        )),
    ))
}
