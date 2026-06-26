use repose_core::prelude::*;
use repose_material::material3::{Button, ButtonConfig};
use repose_ui::pager::PagerState;
use repose_ui::pager::{HorizontalPager, VerticalPager};
use repose_ui::*;
use std::rc::Rc;

use crate::ui::{Page, Section, sp};

const PAGES: usize = 6;

fn pager_colors() -> [Color; PAGES] {
    [
        Color::from_rgb(0xBB, 0x86, 0xFC),
        Color::from_rgb(0x67, 0xBA, 0x80),
        Color::from_rgb(0x5B, 0xBE, 0xE6),
        Color::from_rgb(0xF2, 0xA9, 0x00),
        Color::from_rgb(0xEA, 0x6A, 0x6A),
        Color::from_rgb(0xCF, 0x93, 0xD9),
    ]
}

fn dots(current: usize) -> View {
    let colors = pager_colors();
    Row(Modifier::new()
        .align_items(AlignItems::Center)
        .padding(sp::SM))
    .child(
        (0..PAGES)
            .map(|i| {
                Box(Modifier::new()
                    .size(8.0, 8.0)
                    .margin(4.0)
                    .background(if i == current {
                        colors[i]
                    } else {
                        theme().outline_variant
                    })
                    .clip_rounded(4.0))
            })
            .collect::<Vec<_>>(),
    )
}

fn page_face(p: usize, hint: &'static str) -> View {
    let color = pager_colors()[p % PAGES];
    Box(Modifier::new()
        .fill_max_size()
        .background(color.with_alpha(48))
        .clip_rounded(24.0))
    .child(
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::Center)
                .align_items(AlignItems::Center),
        )
        .child((
            Text(format!("Page {}", p + 1)).size(48.0).color(color),
            Text(hint)
                .size(16.0)
                .color(theme().on_surface.with_alpha(180)),
        )),
    )
}

fn controls(state: Rc<PagerState>, current: usize) -> View {
    let prev = {
        let st = state.clone();
        move || st.set_page(st.current_page().saturating_sub(1))
    };
    let next = move || state.set_page((state.current_page() + 1).min(PAGES - 1));
    Row(Modifier::new()
        .align_items(AlignItems::Center)
        .padding(sp::SM))
    .child((
        Button(Modifier::new(), prev, ButtonConfig::default(), || Text("Prev")),
        Spacer(),
        Text(format!("Page {} of {}", current + 1, PAGES))
            .size(14.0)
            .color(theme().on_surface),
        Spacer(),
        Button(Modifier::new(), next, ButtonConfig::default(), || Text("Next")),
    ))
}

fn pager_section(title: &str, state: Rc<PagerState>, pager: View) -> View {
    let current = state.current_page();
    Section(
        title,
        Column(Modifier::new().fill_max_size()).child((
            dots(current),
            pager,
            controls(state, current),
        )),
    )
}

pub fn screen() -> View {
    let h_state: Rc<PagerState> = remember_with_key("h_pager", || PagerState::new(PAGES));
    let v_state: Rc<PagerState> = remember_with_key("v_pager", || PagerState::new(PAGES));

    Page(vec![
        pager_section(
            "HorizontalPager",
            h_state.clone(),
            HorizontalPager(
                "h_demo",
                h_state.clone(),
                Modifier::new().fill_max_size().height(200.0).margin(sp::SM),
                |p| page_face(p, "Swipe left/right"),
            ),
        ),
        pager_section(
            "VerticalPager",
            v_state.clone(),
            VerticalPager(
                "v_demo",
                v_state.clone(),
                Modifier::new().fill_max_size().height(300.0).margin(sp::SM),
                |p| page_face(p, "Swipe up/down"),
            ),
        ),
    ])
}
