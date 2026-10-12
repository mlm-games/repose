#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::LazyRowState;
use repose_ui::ViewExt;
use repose_ui::lazy::{LazyColumn, LazyRow};
use repose_ui::lazy_states::{LazyColumnConfig, LazyColumnState, LazyRowConfig};

/// M3 Carousel - a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
/// Configuration for [`Carousel`].
#[derive(Clone, Debug)]
pub struct CarouselConfig {
    pub modifier: Modifier,
}

impl Default for CarouselConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
        }
    }
}

/// M3 Carousel - a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
pub fn Carousel<T, F>(
    items: Vec<T>,
    item_width: f32,
    peek_amount: f32,
    state: Rc<LazyRowState>,
    item_builder: F,
    config: CarouselConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let padded_modifier = config.modifier.padding_values(PaddingValues {
        left: Dp(peek_amount),
        right: Dp(peek_amount),
        top: Dp(0.0),
        bottom: Dp(0.0),
    });

    let count = items.len();
    LazyRow(
        items,
        item_width,
        item_builder,
        LazyRowConfig {
            state,
            modifier: padded_modifier,
            ..Default::default()
        },
    )
    .semantics(Semantics::new(Role::Carousel).with_collection(1, count))
}

pub fn HorizontalUncontainedCarousel<T, F>(
    items: Vec<T>,
    item_width: f32,
    state: Rc<LazyRowState>,
    item_builder: F,
    config: CarouselConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let count = items.len();
    LazyRow(
        items,
        item_width,
        item_builder,
        LazyRowConfig {
            state,
            modifier: config.modifier,
            ..Default::default()
        },
    )
    .semantics(Semantics::new(Role::Carousel).with_collection(1, count))
}

pub fn VerticalCarousel<T, F, K>(
    items: Vec<T>,
    item_height: f32,
    get_key: K,
    item_builder: F,
    state: Rc<LazyColumnState>,
    config: CarouselConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
    K: Fn(&T) -> u64 + 'static,
{
    let count = items.len();
    LazyColumn(
        items,
        item_height,
        get_key,
        item_builder,
        LazyColumnConfig {
            state,
            modifier: config.modifier,
            ..Default::default()
        },
    )
    .semantics(Semantics::new(Role::Carousel).with_collection(count, 1))
}

#[derive(Clone, Debug)]
pub struct MultiBrowseCarouselConfig {
    pub modifier: Modifier,
    pub peek_amount: Dp,
}

impl Default for MultiBrowseCarouselConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            peek_amount: Dp(16.0),
        }
    }
}

pub fn HorizontalMultiBrowseCarousel<T, F>(
    items: Vec<(T, f32)>,
    state: Rc<HorizontalScrollState>,
    item_builder: F,
    config: MultiBrowseCarouselConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let count = items.len();
    let peek = config.peek_amount;
    let row = repose_ui::Row(
        Modifier::new()
            .fill_max_height()
            .padding_values(PaddingValues {
                left: peek,
                right: peek,
                top: Dp::ZERO,
                bottom: Dp::ZERO,
            })
            .gap(peek),
    )
    .with_children(
        items
            .into_iter()
            .enumerate()
            .map(|(i, (item, width))| {
                repose_ui::Box(Modifier::new().fill_max_height().width(Dp(width)))
                    .child(item_builder(item, i))
            })
            .collect::<Vec<_>>(),
    );
    repose_ui::scroll::HorizontalScrollArea(config.modifier, state, row)
        .semantics(Semantics::new(Role::Carousel).with_collection(1, count))
}
