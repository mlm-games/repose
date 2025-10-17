use compose_core::{Color, Rect, Scene, SceneNode};

pub struct Hud {
    pub inspector_enabled: bool,
    pub hovered: Option<Rect>,
    frame_count: u64,
}

impl Hud {
    pub fn new() -> Self {
        Self {
            inspector_enabled: false,
            hovered: None,
            frame_count: 0,
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
        let text = format!("frame: {}", self.frame_count);
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

pub struct Inspector {
    pub hud: Hud,
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
