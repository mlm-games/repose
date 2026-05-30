use repose_core::prelude::*;
use repose_ui::pager::{HorizontalPager, PagerState, VerticalPager};
use repose_ui::*;
use std::rc::Rc;

use crate::ui::Section;

fn pager_colors() -> [Color; 6] {
    [
        Color::from_rgb(0xBB, 0x86, 0xFC),
        Color::from_rgb(0x67, 0xBA, 0x80),
        Color::from_rgb(0x5B, 0xBE, 0xE6),
        Color::from_rgb(0xF2, 0xA9, 0x00),
        Color::from_rgb(0xEA, 0x6A, 0x6A),
        Color::from_rgb(0xCF, 0x93, 0xD9),
    ]
}

pub fn screen() -> View {
    let h_state: Rc<PagerState> = remember_with_key("h_pager", || PagerState::new(6));
    let v_state: Rc<PagerState> = remember_with_key("v_pager", || PagerState::new(6));
    let colors = pager_colors();

    let h_page = h_state.current_page();
    let v_page = v_state.current_page();

    Column(Modifier::new().fill_max_width().gap(24.0)).child((
        Section("HorizontalPager", {
            Column(Modifier::new().fill_max_size()).child((
                Row(Modifier::new().align_items(AlignItems::Center).padding(8.0)).child(
                    (0..6)
                        .map(|i| {
                            Box(Modifier::new()
                                .size(8.0, 8.0)
                                .margin(4.0)
                                .background(
                                    if i == h_page {
                                        colors[i]
                                    } else {
                                        theme().outline_variant
                                    },
                                )
                                .clip_rounded(4.0))
                        })
                        .collect::<Vec<_>>(),
                ),
                HorizontalPager(
                    "h_demo",
                    h_state.clone(),
                    Modifier::new().fill_max_size().height(200.0).margin(8.0),
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
                                Text(format!("Page {}", p + 1)).size(48.0).color(color),
                                Text("Swipe left/right").size(16.0).color(
                                    theme().on_surface.with_alpha(180),
                                ),
                            )),
                        )
                    },
                ),
                Row(Modifier::new().align_items(AlignItems::Center).padding(8.0)).child((
                    Button(Text("Prev"), {
                        let st = h_state.clone();
                        move || {
                            let p = st.current_page().saturating_sub(1);
                            st.set_page(p);
                        }
                    }),
                    Spacer(),
                    Text(format!("Page {} of 6", h_page + 1))
                        .size(14.0)
                        .color(theme().on_surface),
                    Spacer(),
                    Button(Text("Next"), {
                        let st = h_state.clone();
                        move || {
                            let p = (st.current_page() + 1).min(5);
                            st.set_page(p);
                        }
                    }),
                )),
            ))
        }),
        Section("VerticalPager", {
            Column(Modifier::new().fill_max_size()).child((
                Row(Modifier::new().align_items(AlignItems::Center).padding(8.0)).child(
                    (0..6)
                        .map(|i| {
                            Box(Modifier::new()
                                .size(8.0, 8.0)
                                .margin(4.0)
                                .background(
                                    if i == v_page {
                                        colors[i]
                                    } else {
                                        theme().outline_variant
                                    },
                                )
                                .clip_rounded(4.0))
                        })
                        .collect::<Vec<_>>(),
                ),
                VerticalPager(
                    "v_demo",
                    v_state.clone(),
                    Modifier::new().fill_max_size().height(300.0).margin(8.0),
                    move |p| {
                        let color: Color = colors[p % colors.len()];
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
                                Text(format!("Page {}", p + 1)).size(48.0).color(color),
                                Text("Swipe up/down").size(16.0).color(
                                    theme().on_surface.with_alpha(180),
                                ),
                            )),
                        )
                    },
                ),
                Row(Modifier::new().align_items(AlignItems::Center).padding(8.0)).child((
                    Button(Text("Prev"), {
                        let st = v_state.clone();
                        move || {
                            let p = st.current_page().saturating_sub(1);
                            st.set_page(p);
                        }
                    }),
                    Spacer(),
                    Text(format!("Page {} of 6", v_page + 1))
                        .size(14.0)
                        .color(theme().on_surface),
                    Spacer(),
                    Button(Text("Next"), {
                        let st = v_state.clone();
                        move || {
                            let p = (st.current_page() + 1).min(5);
                            st.set_page(p);
                        }
                    }),
                )),
            ))
        }),
    ))
}
