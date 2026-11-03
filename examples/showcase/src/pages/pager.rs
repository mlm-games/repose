use repose_core::PaddingValues;
use repose_core::prelude::*;
use repose_material::material3::{Button, ButtonConfig};
use repose_ui::pager::{HorizontalPager, PagerConfig, PagerState, VerticalPager};
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

fn dots(state: &PagerState) -> View {
    let colors = pager_colors();
    let position = state.position();
    FlowRow(
        Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER)
            .padding(sp::SM),
        FlowRowConfig::default(),
    )
    .child(
        (0..PAGES)
            .map(|i| {
                let selected = (1.0 - (i as f32 - position).abs()).clamp(0.0, 1.0);
                let color = theme().outline_variant.interpolate(&colors[i], selected);
                let size = 8.0 + 4.0 * selected;
                Box(Modifier::new()
                    .size(Dp(size), Dp(size))
                    .margin(Dp(4.0))
                    .background(color)
                    .clip_rounded(Dp(size / 2.0)))
            })
            .collect::<Vec<_>>(),
    )
}

fn page_face(p: usize, hint: &'static str) -> View {
    let color = pager_colors()[p % PAGES];
    Box(Modifier::new()
        .fill_max_size()
        .background(color.with_alpha(48))
        .clip_rounded(Dp(24.0)))
    .child(
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
        )
        .child((
            Text(format!("Page {}", p + 1)).size(Sp(48.0)).color(color),
            Text(hint)
                .size(Sp(16.0))
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
    FlowRow(
        Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER)
            .padding(sp::SM),
        FlowRowConfig::default(),
    )
    .child((
        Button(Modifier::new(), prev, ButtonConfig::default(), || {
            Text("Prev")
        }),
        Spacer(),
        Text(format!("Page {} of {}", current + 1, PAGES))
            .size(Sp(14.0))
            .color(theme().on_surface),
        Spacer(),
        Button(Modifier::new(), next, ButtonConfig::default(), || {
            Text("Next")
        }),
    ))
}

fn pager_section(title: &str, state: Rc<PagerState>, pager: View) -> View {
    let current = state.current_page();
    Section(
        title,
        Column(Modifier::new().fill_max_size()).child((
            dots(&state),
            pager,
            controls(state, current),
        )),
    )
}

pub fn screen() -> View {
    let h_state: Rc<PagerState> = remember_with_key("h_pager", || PagerState::new(PAGES));
    let v_state: Rc<PagerState> = remember_with_key("v_pager", || PagerState::new(PAGES));
    let peek_state: Rc<PagerState> = remember_with_key("peek_pager", || PagerState::new(PAGES));

    Page(vec![
        pager_section(
            "HorizontalPager",
            h_state.clone(),
            HorizontalPager(
                "h_demo",
                h_state.clone(),
                |p| page_face(p, "Drag, then release to snap"),
                PagerConfig {
                    modifier: Modifier::new()
                        .fill_max_size()
                        .height(Dp(200.0))
                        .margin(sp::SM),
                    ..Default::default()
                },
            ),
        ),
        pager_section(
            "HorizontalPager peek",
            peek_state.clone(),
            HorizontalPager(
                "peek_demo",
                peek_state.clone(),
                |p| page_face(p, "content_padding + page_spacing"),
                PagerConfig {
                    modifier: Modifier::new()
                        .fill_max_size()
                        .height(Dp(180.0))
                        .margin(sp::SM),
                    page_spacing: Dp(12.0),
                    content_padding: PaddingValues {
                        left: Dp(32.0),
                        right: Dp(32.0),
                        top: Dp::ZERO,
                        bottom: Dp::ZERO,
                    },
                    ..Default::default()
                },
            ),
        ),
        pager_section(
            "VerticalPager",
            v_state.clone(),
            VerticalPager(
                "v_demo",
                v_state.clone(),
                |p| page_face(p, "Drag up/down to flip"),
                PagerConfig {
                    modifier: Modifier::new()
                        .fill_max_size()
                        .height(Dp(260.0))
                        .margin(sp::SM),
                    ..Default::default()
                },
            ),
        ),
    ])
}
