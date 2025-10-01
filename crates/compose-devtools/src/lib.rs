use compose_core::{Scene, SceneNode, Color, Rect};

pub struct Hud {
    frame_count: u64,
}

impl Hud {
    pub fn new() -> Self { Self { frame_count: 0 } }
    pub fn overlay(&mut self, scene: &mut Scene) {
        self.frame_count += 1;
        let text = format!("fps-ish frame: {}", self.frame_count);
        scene.nodes.push(SceneNode::Text {
            rect: Rect { x: 8.0, y: 8.0, w: 200.0, h: 16.0 },
            text, color: Color::from_hex("#AAAAAA"), size: 14.0
        });
    }
}
