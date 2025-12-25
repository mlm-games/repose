use std::rc::Rc;

use repose_core::{TextDirection, prelude::*, signal, with_text_direction};
use repose_navigation::{
    NavDisplay, NavTransition, Navigator, back, remember_back_stack, renderer,
};
use serde::{Deserialize, Serialize};

use crate::{pages, ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Home,
    Layout,
    Widgets,
    Text,
    Scroll,
    Canvas,
    Animation,
    Lists,
    Errors,
}

impl Route {
    pub fn title(self) -> &'static str {
        match self {
            Route::Home => "Home",
            Route::Layout => "Layout",
            Route::Widgets => "Widgets",
            Route::Text => "Text",
            Route::Scroll => "Scroll",
            Route::Canvas => "Canvas",
            Route::Lists => "Lists",
            Route::Animation => "Animation",
            Route::Errors => "Errors",
        }
    }

    pub fn id(self) -> u64 {
        match self {
            Route::Home => 1,
            Route::Layout => 2,
            Route::Widgets => 3,
            Route::Text => 4,
            Route::Scroll => 5,
            Route::Canvas => 6,
            Route::Lists => 7,
            Route::Animation => 8,
            Route::Errors => 9,
        }
    }
}

pub fn app(_s: &mut Scheduler) -> View {
    // App state
    let dark = remember(|| signal(true));
    let rtl = remember(|| signal(false));
    let density = remember(|| signal(1.0f32));
    let text_scale = remember(|| signal(1.0f32));

    // Theme presets
    let theme_light = {
        let mut t = Theme::default();
        t.background = Color::from_hex("#FAFAFA");
        t.surface = Color::from_hex("#FFFFFF");
        t.on_surface = Color::from_hex("#222222");
        t.primary = Color::from_hex("#3B82F6");
        t.on_primary = Color::WHITE;
        t.outline = Color::from_hex("#DDDDDD");
        t.focus = Color::from_hex("#2563EB");
        t.button_bg = Color::from_hex("#3B82F6");
        t.button_bg_hover = Color::from_hex("#2563EB");
        t.button_bg_pressed = Color::from_hex("#1D4ED8");
        t.scrollbar_track = Color(0, 0, 0, 20);
        t.scrollbar_thumb = Color(0, 0, 0, 80);
        t
    };
    let theme_dark = Theme::default();

    let stack = remember_back_stack(Route::Home);
    let navigator = Navigator {
        stack: (*stack).clone(),
    };

    // Back handler: keep it simple and robust; set each frame.
    back::set(Some(Rc::new({
        let nav = navigator.clone();
        move || nav.pop()
    })));

    let current = stack
        .top()
        .map(|(_, k, _saved, _scope)| k)
        .unwrap_or(Route::Home);

    // Typed route -> page renderer
    let render = renderer(move |scope| match *scope.key() {
        Route::Home => pages::home::screen(),
        Route::Layout => pages::layout::screen(),
        Route::Widgets => pages::widgets::screen(),
        Route::Text => pages::text::screen(),
        Route::Scroll => pages::scroll::screen(),
        Route::Lists => pages::lists::screen(),
        Route::Canvas => pages::canvas::screen(),
        Route::Animation => pages::animation::screen(),
        Route::Errors => pages::errors::screen(),
    });

    let dir = if rtl.get() {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    };

    let chosen_theme = if dark.get() { theme_dark } else { theme_light };

    with_text_direction(dir, || {
        with_theme(chosen_theme, || {
            with_density(
                Density {
                    scale: density.get(),
                },
                || {
                    with_text_scale(TextScale(text_scale.get()), || {
                        ui::AppShell(
                            current,
                            navigator.clone(),
                            dark.get(),
                            {
                                let dark = dark.clone();
                                move |v| dark.set(v)
                            },
                            rtl.get(),
                            {
                                let rtl = rtl.clone();
                                move |v| rtl.set(v)
                            },
                            density.get(),
                            {
                                let density = density.clone();
                                move |v| density.set(v.clamp(0.75, 2.0))
                            },
                            text_scale.get(),
                            {
                                let text_scale = text_scale.clone();
                                move |v| text_scale.set(v.clamp(0.75, 2.0))
                            },
                            {
                                // Content
                                let transition = NavTransition::default();
                                NavDisplay(stack.clone(), render.clone(), None, transition)
                            },
                        )
                    })
                },
            )
        })
    })
}
