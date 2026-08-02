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

            Self::draw_fps_sparkline(
                scene,
                bar_x + 2.0,
                bar_y + bar_h + 4.0,
                bar_w - 4.0,
                16.0,
                &self.fps_history,
                self.fps_history_idx,
            );

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

            let mut text_y = bar_y + bar_h + 24.0;
            Self::push_text(scene, bar_x, text_y, 100.0, &format!("{:.0} fps", self.fps_smooth), "#AAAAAA", 12.0);
            text_y += 16.0;

            Self::push_text(scene, bar_x, text_y, 100.0, &format!("frame: {}", self.frame_count), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 120.0, &format!("build: {:.1}ms", m.build_ms), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 120.0, &format!("layout: {:.1}ms", m.layout_ms), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 120.0, &format!("paint: {:.1}ms", m.paint_ms), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 120.0, &format!("widgets: {}", m.widget_count), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 120.0, &format!("signals: {}", m.signal_count), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(scene, bar_x, text_y, 140.0, &format!("scene: {}", m.scene_nodes), "#888888", 11.0);
            text_y += 14.0;
            Self::push_text(
                scene,
                bar_x,
                text_y,
                200.0,
                &format!(
                    "taffy: {:+}/{:+}",
                    m.taffy_created, m.taffy_reused
                ),
                "#888888",
                11.0,
            );
            text_y += 14.0;
            Self::push_text(
                scene,
                bar_x,
                text_y,
                200.0,
                &format!("layout: {}h/{}m", m.layout_hits, m.layout_misses),
                "#888888",
                11.0,
            );
            text_y += 14.0;
            Self::push_text(
                scene,
                bar_x,
                text_y,
                200.0,
                &format!(
                    "paint cache: {}h/{}m ({} culled)",
                    m.paint_cache_hits, m.paint_cache_misses, m.paint_culled
                ),
                "#888888",
                11.0,
            );
            text_y += 14.0;

            if let Some(hover) = &self.hovered_semantics {
                text_y += 6.0;
                Self::push_text(
                    scene,
                    bar_x,
                    text_y,
                    200.0,
                    &format!("↳ {}: {:?}", hover.id, hover.role),
                    "#44AAFF",
                    11.0,
                );
                if let Some(lbl) = &hover.label {
                    text_y += 14.0;
                    Self::push_text(
                        scene,
                        bar_x,
                        text_y,
                        200.0,
                        &format!("  \"{}\"", lbl),
                        "#66CCFF",
                        10.0,
                    );
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

    /// Push a single HUD text line.
    fn push_text(scene: &mut Scene, x: f32, y: f32, w: f32, txt: &str, color: &str, size: f32) {
        scene.nodes.push(SceneNode::Text {
            rect: Rect {
                x,
                y,
                w,
                h: 14.0,
            },
            text: Arc::<str>::from(txt.to_string()),
            color: Color::from_hex(color),
            size,
            font_family: None,
            text_align: TextAlign::Unspecified,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            text_decoration: TextDecoration::default(),
            letter_spacing: 0.0,
            line_height: 0.0,
            extra_style: Default::default(),
            url: None,
            font_variation_settings: None,
        });
    }

    /// Draw a horizontal bar-chart sparkline from a rolling FPS history.
    ///
    /// The newest sample is drawn at the right edge; older samples scroll left.
    fn draw_fps_sparkline(
        scene: &mut Scene,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        history: &[f32],
        idx: usize,
    ) {
        let n = history.len();
        if n == 0 || idx == 0 {
            return;
        }
        scene.nodes.push(SceneNode::Rect {
            rect: Rect { x, y, w, h },
            brush: Brush::Solid(Color::from_hex("#1A1A1ACC")),
            radius: [2.0; 4],
        });
        let bin_w = w / n as f32;
        let max_fps = 60.0f32.max(history.iter().copied().fold(0.0f32, f32::max));
        for i in 0..n {
            // Walk oldest->newest: idx points one past the newest sample.
            let sample = history[(i + idx) % n];
            let frac = (sample / max_fps).min(1.0);
            let bh = (h - 2.0) * frac;
            let color = if frac >= 0.83 {
                "#44FF44"
            } else if frac >= 0.5 {
                "#FFAA00"
            } else {
                "#FF4444"
            };
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: x + i as f32 * bin_w,
                    y: y + (h - 2.0) - bh,
                    w: (bin_w - 1.0).max(0.5),
                    h: bh.max(1.0),
                },
                brush: Brush::Solid(Color::from_hex(color)),
                radius: [0.0; 4],
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub build_ms: f32,
    pub layout_ms: f32,
    pub paint_ms: f32,
    pub scene_nodes: usize,
    pub widget_count: usize,
    pub signal_count: usize,
    // Layout engine counters.
    pub taffy_created: usize,
    pub taffy_reused: usize,
    pub layout_hits: usize,
    pub layout_misses: usize,
    pub paint_cache_hits: usize,
    pub paint_cache_misses: usize,
    pub paint_culled: usize,
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
