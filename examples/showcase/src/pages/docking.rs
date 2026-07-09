use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_docking::*;
use repose_material::{Icon, Symbol, material_symbols};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;

use crate::ui::{Caption, Hint, Page, Section, sp};

material_symbols! {
    folder    : '\u{E2C7}',
    image     : '\u{E3F4}',
    music_note: '\u{E405}',
    videocam  : '\u{E04B}',
    tune      : '\u{E429}',
}

fn inspector_panel() -> View {
    let th = theme();
    let field = |label: &str, value: &str| {
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER)
            .padding(sp::SM))
        .child((
            Text(label.to_string())
                .size(12.0)
                .color(th.on_surface_variant),
            Spacer(),
            Text(value.to_string()).size(12.0).color(th.on_surface),
        ))
    };

    ScrollArea(
        Modifier::new().fill_max_size(),
        remember_scroll_state("dock:inspector"),
        Column(Modifier::new().fill_max_width().padding(sp::SM).gap(sp::SM)).child((
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                Icon(Symbols::tune).size(16.0).color(th.primary),
                Text("Selected: Layer 3").size(13.0).color(th.on_surface),
            )),
            Box(Modifier::new()
                .fill_max_width()
                .background(th.surface_container)
                .clip_rounded(sp::SM))
            .child(Column(Modifier::new().fill_max_width()).child((
                field("Position X", "128"),
                field("Position Y", "340"),
                field("Width", "220"),
                field("Height", "160"),
                field("Rotation", "0°"),
                field("Opacity", "100%"),
            ))),
            Caption("Drag this tab to dock it elsewhere."),
        )),
    )
}

fn assets_panel() -> View {
    let th = theme();
    struct Asset { name: &'static str, glyph: Symbol }
    const ASSETS: &[Asset] = &[
        Asset { name: "hero.png", glyph: Symbols::image },
        Asset { name: "logo.svg", glyph: Symbols::image },
        Asset { name: "intro.mp4", glyph: Symbols::videocam },
        Asset { name: "theme.mp3", glyph: Symbols::music_note },
        Asset { name: "bg-01.png", glyph: Symbols::image },
        Asset { name: "bg-02.png", glyph: Symbols::image },
        Asset { name: "outro.mp4", glyph: Symbols::videocam },
        Asset { name: "pad.wav", glyph: Symbols::music_note },
    ];

    ScrollArea(
        Modifier::new().fill_max_size(),
        remember_scroll_state("dock:assets"),
        Column(Modifier::new().fill_max_width().padding(sp::SM).gap(4.0)).child(
            ASSETS
                .iter()
                .enumerate()
                .map(|(i, asset)| {
                    Row(Modifier::new()
                        .key(i as u64)
                        .fill_max_width()
                        .padding(sp::SM)
                        .background(th.surface_container)
                        .clip_rounded(sp::SM)
                        .align_items(AlignItems::CENTER)
                        .gap(sp::SM))
                    .child((
                        Icon(asset.glyph).size(16.0).color(th.on_surface_variant),
                        Text(asset.name.to_string()).size(12.0).color(th.on_surface),
                        Spacer(),
                        Text(format!("{} KB", 12 + i * 47))
                            .size(11.0)
                            .color(th.on_surface_variant),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
    )
}

fn scene_panel() -> View {
    let th = theme();
    Box(Modifier::new()
        .fill_max_size()
        .background(th.surface_container_lowest))
    .child(Column(Modifier::new().fill_max_size()).child((
        Row(Modifier::new()
            .fill_max_width()
            .padding(sp::SM)
            .background(th.surface_container)
            .align_items(AlignItems::CENTER)
            .gap(sp::SM))
        .child((
            Icon(Symbols::folder).size(14.0).color(th.on_surface_variant),
            Text("scene / main").size(12.0).color(th.on_surface_variant),
            Spacer(),
            Text("1920 × 1080").size(11.0).color(th.on_surface_variant),
        )),
        Box(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER))
        .child(Column(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
            Box(Modifier::new()
                .size(120.0, 68.0)
                .background(th.primary.with_alpha(40))
                .border(1.0, th.primary, sp::SM)
                .clip_rounded(sp::SM)),
            Text("Scene viewport")
                .size(12.0)
                .color(th.on_surface_variant),
        ))),
    )))
}

fn panel(id: u64, title: &str, body: Rc<dyn Fn() -> View>) -> DockPanel {
    DockPanel {
        id,
        title: title.to_string(),
        content: body,
    }
}

pub fn screen() -> View {
    let state = remember_with_key("dock:state", || {
        RefCell::new(DockState::new_with_tabs(vec![1, 2, 3]))
    });

    let panels = vec![
        panel(1, "Inspector", Rc::new(inspector_panel)),
        panel(2, "Assets", Rc::new(assets_panel)),
        panel(3, "Scene", Rc::new(scene_panel)),
    ];

    Page(vec![Section(
        "Docking Workspace",
        Column(Modifier::new().gap(sp::MD)).child((
            Hint("Drag tabs to the edges or center of another panel to dock. Drag splitters to resize."),
            DockArea(
                "showcase_dock",
                Modifier::new()
                    .height(460.0)
                    .fill_max_width()
                    .border(1.0, theme().outline_variant, sp::LG)
                    .clip_rounded(sp::LG),
                state,
                panels,
                DockCallbacks::default(),
            ),
        )),
    )])
}