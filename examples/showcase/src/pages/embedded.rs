use repose_canvas::Embedded;
use repose_core::PaintCallbackInfo;
use repose_core::prelude::*;
use repose_render_wgpu::{Callback, CallbackResources, WgpuCallback};
use repose_ui::*;

use crate::ui::{Hint, Page, Section, sp};

struct DemoTriangle {
    angle: f32,
}

impl WgpuCallback for DemoTriangle {
    fn paint(
        &self,
        info: PaintCallbackInfo,
        _rpass: &mut wgpu::RenderPass<'static>,
        _resources: &CallbackResources,
    ) {
        // For this showcase page we just log to prove the callback fires.
        log::info!(
            "DemoTriangle paint: viewport {:?} clip {:?} angle {}",
            info.viewport,
            info.clip_rect,
            self.angle
        );
    }
}

pub fn screen() -> View {
    let angle = remember_mutable(|| 0.0_f32);
    let drag_pos = remember_mutable(|| Vec2 { x: 0.0, y: 0.0 });

    let payload = {
        let a = *angle.get();
        Callback::new(DemoTriangle { angle: a })
    };

    Page(vec![
        Section(
            "Embedded GPU (egui PaintCallback-like)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Text("This box is an Embedded view. It reserves layout space via Taffy, participates in hit-testing, and invokes a wgpu callback inside the main repose render pass.")
                    .color(theme().on_surface_variant)
                    .size(12.0),
            )),
        ),
        Section(
            "Interactive 3D placeholder",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Drag inside the box to update angle state (PointerEvent forwarding). The callback itself would render a triangle at that angle in a real app."),
                Row(Modifier::new().gap(sp::SM)).child((
                    Text(format!("angle = {:.2} rad", *angle.get())),
                    Text(format!("drag pos = ({:.0}, {:.0})", drag_pos.get().x, drag_pos.get().y))
                        .color(theme().on_surface_variant)
                        .size(12.0),
                )),
                Embedded(
                    Modifier::new()
                        .size(560.0, 220.0)
                        .background(theme().surface_container_low)
                        .border(1.0, theme().outline_variant, 16.0)
                        .clip_rounded(16.0)
                        .on_pointer_down({
                            let angle = angle.clone();
                            let drag_pos = drag_pos.clone();
                            move |ev: PointerEvent| {
                                drag_pos.set(ev.position);
                                log::info!("down {:?}", ev.position);
                                let _ = &angle;
                            }
                        })
                        .on_pointer_move({
                            let angle = angle.clone();
                            let drag_pos = drag_pos.clone();
                            move |ev: PointerEvent| {
                                // ev.position is local to the Embedded rect; use delta-like
                                let dx = ev.position.x - drag_pos.get().x;
                                drag_pos.set(ev.position);
                                angle.update(|a| *a += dx * 0.01);
                                request_frame();
                            }
                        }),
                    payload,
                ),
                Text("Code:")
                    .color(theme().on_surface_variant)
                    .size(11.0),
                Box(Modifier::new()
                    .background(theme().surface_container)
                    .padding(sp::SM)
                    .clip_rounded(8.0)
                ).child(
                    Text(r#"let payload = Callback::new(MyTriangle { angle });
Embedded(Modifier::new().size(300.0,300.0)
    .on_pointer_move(|ev| angle += ev.position.x * 0.01),
    payload)"#)
                ),
            )),
        ),

    ])
}
