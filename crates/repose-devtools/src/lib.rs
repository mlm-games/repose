use std::sync::Arc;

use web_time::Instant;

use repose_core::{
    Brush, Color, FontStyle, FontWeight, Rect, Scene, SceneNode, TextAlign, TextDecoration,
};

const FPS_HISTORY_LEN: usize = 60;

pub struct Hud {
    pub inspector_enabled: bool,
    pub hovered: Option<Rect>,
    pub hovered_semantics: Option<HoveredInfo>,
    frame_count: u64,
    last_frame: Option<Instant>,
    fps_smooth: f32,
    fps_history: [f32; FPS_HISTORY_LEN],
    fps_history_idx: usize,
    pub metrics: Option<Metrics>,
    selected_widget: Option<SelectedWidget>,
}

#[derive(Clone, Debug)]
pub struct HoveredInfo {
    pub id: u64,
    pub role: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SelectedWidget {
    pub id: u64,
    pub role: String,
    pub label: Option<String>,
    pub bounds: Rect,
}

impl Default for Hud {
    fn default() -> Self {
        Self::new()
    }
}

impl Hud {
    pub fn new() -> Self {
        Self {
            inspector_enabled: false,
            hovered: None,
            hovered_semantics: None,
            frame_count: 0,
            last_frame: None,
            fps_smooth: 0.0,
            fps_history: [0.0; FPS_HISTORY_LEN],
            fps_history_idx: 0,
            metrics: None,
            selected_widget: None,
        }
    }
    pub fn toggle_inspector(&mut self) {
        self.inspector_enabled = !self.inspector_enabled;
    }
    pub fn set_hovered(&mut self, r: Option<Rect>, info: Option<HoveredInfo>) {
        self.hovered = r;
        self.hovered_semantics = info;
    }
    pub fn select_widget(&mut self, info: SelectedWidget) {
        self.selected_widget = Some(info);
    }
    pub fn clear_selection(&mut self) {
        self.selected_widget = None;
    }

    fn update_fps(&mut self, now: Instant) {
        if let Some(prev) = self.last_frame.replace(now) {
            let dt = (now - prev).as_secs_f32();
            if dt > 0.0 && dt < 1.0 {
                let fps = 1.0 / dt;
                let a = 0.3;
                self.fps_smooth = if self.fps_smooth == 0.0 {
                    fps
                } else {
                    (1.0 - a) * self.fps_smooth + a * fps
                };
                self.fps_history[self.fps_history_idx] = fps;
                self.fps_history_idx = (self.fps_history_idx + 1) % FPS_HISTORY_LEN;
            }
        }
    }

    pub fn overlay(&mut self, scene: &mut Scene) {
        self.frame_count += 1;
        self.update_fps(Instant::now());

        let bar_x = 8.0;
        let bar_y = 8.0;
        let bar_w = 120.0;
        let bar_h = 24.0;

        if let Some(m) = &self.metrics {
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: bar_x,
                    y: bar_y,
                    w: bar_w,
                    h: bar_h,
                },
                brush: Brush::Solid(Color::from_hex("#1A1A1ACC")),
                radius: [4.0; 4],
            });

            let fps_norm = (self.fps_smooth / 60.0).min(1.0);
            let bar_fill = bar_w * fps_norm;
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: bar_x + 2.0,
                    y: bar_y + 2.0,
                    w: bar_fill,
                    h: bar_h - 4.0,
                },
                brush: Brush::Solid(if self.fps_smooth >= 50.0 {
                    Color::from_hex("#44FF44")
                } else if self.fps_smooth >= 30.0 {
                    Color::from_hex("#FFAA00")
                } else {
                    Color::from_hex("#FF4444")
                }),
                radius: [2.0; 4],
            });

            let mut text_y = bar_y + bar_h + 4.0;
            let line = format!("{:.0} fps", self.fps_smooth);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#AAAAAA"),
                size: 12.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 16.0;

            let line = format!("frame: {}", self.frame_count);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 14.0;

            let line = format!("build: {:.1}ms", m.build_ms);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 14.0;

            let line = format!("layout: {:.1}ms", m.layout_ms);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 14.0;

            let line = format!("widgets: {}", m.widget_count);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 14.0;

            let line = format!("signals: {}", m.signal_count);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 80.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });
            text_y += 14.0;

            let line = format!("scene nodes: {}", m.scene_nodes);
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: bar_x,
                    y: text_y,
                    w: 100.0,
                    h: 14.0,
                },
                text: Arc::<str>::from(line),
                color: Color::from_hex("#888888"),
                size: 11.0,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
            });

            if let Some(hover) = &self.hovered_semantics {
                text_y += 20.0;
                let line = format!("↳ {}: {:?}", hover.id, hover.role);
                scene.nodes.push(SceneNode::Text {
                    rect: Rect {
                        x: bar_x,
                        y: text_y,
                        w: 150.0,
                        h: 14.0,
                    },
                    text: Arc::<str>::from(line),
                    color: Color::from_hex("#44AAFF"),
                    size: 11.0,
                    font_family: None,
                    text_align: TextAlign::Unspecified,
                    font_weight: FontWeight::NORMAL,
                    font_style: FontStyle::Normal,
                    text_decoration: TextDecoration::default(),
                    letter_spacing: 0.0,
                    line_height: 0.0,
                    extra_style: Default::default(),
                    url: None,
                });
                if let Some(lbl) = &hover.label {
                    text_y += 14.0;
                    scene.nodes.push(SceneNode::Text {
                        rect: Rect {
                            x: bar_x,
                            y: text_y,
                            w: 150.0,
                            h: 14.0,
                        },
                        text: Arc::<str>::from(format!("  \"{}\"", lbl)),
                        color: Color::from_hex("#66CCFF"),
                        size: 10.0,
                        font_family: None,
                        text_align: TextAlign::Unspecified,
                        font_weight: FontWeight::NORMAL,
                        font_style: FontStyle::Normal,
                        text_decoration: TextDecoration::default(),
                        letter_spacing: 0.0,
                        line_height: 0.0,
                        extra_style: Default::default(),
                        url: None,
                    });
                }
            }
        }

        if let Some(r) = self.hovered {
            scene.nodes.push(SceneNode::Border {
                rect: r,
                color: Color::from_hex("#44AAFF"),
                width: 2.0,
                radius: [2.0; 4],
            });
        }

        if let Some(sel) = &self.selected_widget {
            scene.nodes.push(SceneNode::Border {
                rect: sel.bounds,
                color: Color::from_hex("#FFAA00"),
                width: 2.0,
                radius: [2.0; 4],
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub build_ms: f32,
    pub layout_ms: f32,
    pub scene_nodes: usize,
    pub widget_count: usize,
    pub signal_count: usize,
}

pub struct Inspector {
    pub hud: Hud,
}
impl Default for Inspector {
    fn default() -> Self {
        Self::new()
    }
}

impl Inspector {
    pub fn new() -> Self {
        Self { hud: Hud::new() }
    }
    pub fn frame(&mut self, scene: &mut Scene) {
        if self.hud.inspector_enabled {
            self.hud.overlay(scene);
        }
    }
}
