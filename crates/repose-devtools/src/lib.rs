use web_time::Instant;

use repose_core::{Color, Rect, Scene, SceneNode};

pub struct Hud {
    pub inspector_enabled: bool,
    pub hovered: Option<Rect>,
    frame_count: u64,
    last_frame: Option<Instant>,
    fps_smooth: f32,
    pub metrics: Option<Metrics>,
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
            frame_count: 0,
            last_frame: None,
            fps_smooth: 0.0,
            metrics: None,
        }
    }
    pub fn toggle_inspector(&mut self) {
        self.inspector_enabled = !self.inspector_enabled;
    }
    pub fn set_hovered(&mut self, r: Option<Rect>) {
        self.hovered = r;
    }

    pub fn overlay(&mut self, scene: &mut Scene) {
        self.frame_count += 1;
        // FPS
        let now = Instant::now();
        if let Some(prev) = self.last_frame.replace(now) {
            let dt = (now - prev).as_secs_f32();
            if dt > 0.0 {
                let fps = 1.0 / dt;
                // simple EMA
                let a = 0.2;
                self.fps_smooth = if self.fps_smooth == 0.0 {
                    fps
                } else {
                    (1.0 - a) * self.fps_smooth + a * fps
                };
            }
        }
        let mut lines = vec![
            format!("frame: {}", self.frame_count),
            format!("fps: {:.1}", self.fps_smooth),
        ];
        if let Some(m) = &self.metrics {
            lines.push(format!("build+layout: {:.2} ms", m.build_layout_ms));
            lines.push(format!("nodes: {}", m.scene_nodes));
        }
        let text = lines.join("  |  ");
        scene.nodes.push(SceneNode::Text {
            rect: Rect {
                x: 8.0,
                y: 8.0,
                w: 200.0,
                h: 16.0,
            },
            text,
            color: Color::from_hex("#AAAAAA"),
            size: 14.0,
        });

        if let Some(r) = self.hovered {
            scene.nodes.push(SceneNode::Border {
                rect: r,
                color: Color::from_hex("#44AAFF"),
                width: 2.0,
                radius: 0.0,
            });
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub build_layout_ms: f32,
    pub scene_nodes: usize,
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
