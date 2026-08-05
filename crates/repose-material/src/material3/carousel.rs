#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::LazyRowState;
use repose_ui::lazy::LazyRow;
use repose_ui::lazy_states::LazyRowConfig;


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
        left: peek_amount,
        right: peek_amount,
        top: 0.0,
        bottom: 0.0,
    });

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
}
