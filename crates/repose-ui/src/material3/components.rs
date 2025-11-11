use std::rc::Rc;

use crate::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt, anim::animate_f32};
use repose_core::*;

use crate::{Stack, Surface};

pub fn AlertDialog(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
) -> View {
    if !visible {
        return Box(Modifier::new());
    }

    Stack(Modifier::new().fill_max_size()).child((
        // Scrim
        Box(Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#000000AA"))
            .clickable()
            .on_pointer_down(move |_| on_dismiss())),
        // Dialog content
        Surface(
            Modifier::new()
                .size(280.0, 200.0)
                .background(theme().surface)
                .clip_rounded(28.0)
                .padding(24.0),
            Column(Modifier::new()).child((
                title,
                Box(Modifier::new().size(1.0, 16.0)),
                text,
                Spacer(),
                Row(Modifier::new()).child((
                    dismiss_button.unwrap_or(Box(Modifier::new())),
                    Spacer(),
                    confirm_button,
                )),
            )),
        ),
    ))
}

pub fn BottomSheet(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
) -> View {
    let offset = animate_f32(
        "sheet_offset",
        if visible { 0.0 } else { 800.0 },
        AnimationSpec::spring_gentle(),
    );

    Stack(Modifier::new().fill_max_size()).child((
        // Scrim
        if visible {
            Box(Modifier::new()
                .fill_max_size()
                .background(Color::from_hex("#00000055"))
                .on_pointer_down(move |_| on_dismiss()))
        } else {
            Box(Modifier::new())
        },
        // Sheet
        Box(modifier
            .absolute()
            .offset(None, Some(offset), Some(0.0), Some(0.0)))
        .child(content),
    ))
}

pub fn NavigationBar(selected_index: usize, items: Vec<NavItem>) -> View {
    Row(Modifier::new()
        .fill_max_size()
        .background(theme().surface)
        .padding(8.0))
    .child(
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| NavigationBarItem(item, i == selected_index))
            .collect::<Vec<_>>(),
    )
}

pub struct NavItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

fn NavigationBarItem(item: NavItem, selected: bool) -> View {
    let color = if selected {
        theme().primary
    } else {
        theme().on_surface
    };

    Column(
        Modifier::new()
            .flex_grow(1.0)
            .clickable()
            .on_pointer_down(move |_| (item.on_click)()),
    )
    .child((
        item.icon, // Tint with color
        Text(item.label).color(color),
    ))
}

pub fn Card(modifier: Modifier, elevated: bool, content: View) -> View {
    Surface(
        modifier
            .background(theme().surface)
            .border(1.0, Color::from_hex("#22222222"), 12.0)
            .clip_rounded(12.0)
            .padding(16.0),
        content,
    )
}

pub fn OutlinedCard(modifier: Modifier, content: View) -> View {
    Surface(
        modifier
            .border(1.0, Color::from_hex("#444444"), 12.0)
            .clip_rounded(12.0)
            .padding(16.0),
        content,
    )
}

pub fn FilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
) -> View {
    let bg = if selected {
        theme().primary
    } else {
        theme().surface
    };
    let fg = if selected {
        theme().on_primary
    } else {
        theme().on_surface
    };

    Surface(
        Modifier::new()
            .background(bg)
            .border(1.0, Color::from_hex("#444444"), 8.0)
            .clip_rounded(8.0)
            .padding(12.0)
            .clickable()
            .on_pointer_down(move |_| on_click()),
        Row(Modifier::new()).child((leading_icon.unwrap_or(Box(Modifier::new())), label)),
    )
}

pub fn Scaffold(
    top_bar: Option<View>,
    bottom_bar: Option<View>,
    floating_action_button: Option<View>,
    content: impl Fn(PaddingValues) -> View,
) -> View {
    Stack(Modifier::new().fill_max_size()).child((
        // Main content with padding
        Box(Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                top: if top_bar.is_some() { 64.0 } else { 0.0 },
                bottom: if bottom_bar.is_some() { 80.0 } else { 0.0 },
                ..Default::default()
            }))
        .child(content(PaddingValues::default())),
        // Top bar
        if let Some(bar) = top_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(0.0), Some(0.0), None))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        // Bottom bar
        if let Some(bar) = bottom_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), None, Some(0.0), Some(0.0)))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        // FAB
        if let Some(fab) = floating_action_button {
            Box(Modifier::new()
                .absolute()
                .offset(None, None, Some(16.0), Some(16.0)))
            .child(fab)
        } else {
            Box(Modifier::new())
        },
    ))
}
