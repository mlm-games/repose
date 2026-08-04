use std::sync::Arc;

use repose_canvas::Canvas;
use repose_core::prelude::*;
use repose_ui::anim::animate_f32_from;
use repose_ui::*;
use web_time::Duration;

use crate::ui::{Hint, Page, Section, sp};

fn rotate_around(pivot: Vec2, angle: f32) -> [f32; 6] {
    let (s, c) = angle.sin_cos();
    let m00 = c;
    let m01 = -s;
    let m10 = s;
    let m11 = c;
    let tx = pivot.x - (m00 * pivot.x + m01 * pivot.y);
    let ty = pivot.y - (m10 * pivot.x + m11 * pivot.y);
    [m00, m01, m10, m11, tx, ty]
}

fn polygon_mesh(pts: &[[f32; 2]], color: Color) -> Arc<VectorMeshData> {
    let linear = color.to_linear();
    let vertices: Arc<[VectorVertex]> = pts
        .iter()
        .map(|p| VectorVertex {
            pos: *p,
            color: linear,
            uv: [0.0; 2],
        })
        .collect();
    let mut indices = Vec::with_capacity((pts.len() - 2) * 3);
    for i in 1..(pts.len() as u32 - 1) {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }
    Arc::new(VectorMeshData {
        vertices,
        indices: indices.into(),
    })
}

fn circle_mesh(cx: f32, cy: f32, r: f32, segments: u32, color: Color) -> Arc<VectorMeshData> {
    let mut pts = Vec::with_capacity(segments as usize);
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        pts.push([cx + r * a.cos(), cy + r * a.sin()]);
    }
    polygon_mesh(&pts, color)
}

fn star_mesh(
    cx: f32,
    cy: f32,
    r_in: f32,
    r_out: f32,
    points: u32,
    color: Color,
) -> Arc<VectorMeshData> {
    let linear = color.to_linear();
    let n = points * 2;
    let mut vertices = vec![VectorVertex {
        pos: [cx, cy],
        color: linear,
        uv: [0.0; 2],
    }];
    for i in 0..n {
        let r = if i % 2 == 0 { r_out } else { r_in };
        let a = i as f32 * std::f32::consts::PI / points as f32;
        vertices.push(VectorVertex {
            pos: [cx + r * a.cos(), cy + r * a.sin()],
            color: linear,
            uv: [0.0; 2],
        });
    }
    let mut indices = Vec::with_capacity(n as usize * 3);
    for i in 0..n {
        indices.push(0);
        indices.push(1 + i);
        indices.push(1 + (i + 1) % n);
    }
    Arc::new(VectorMeshData {
        vertices: vertices.into(),
        indices: indices.into(),
    })
}

pub fn screen() -> View {
    let spin = animate_f32_from(
        "vector_mesh_spin",
        0.0,
        std::f32::consts::TAU,
        AnimationSpec::tween(Duration::from_millis(3600), Easing::EaseInOut)
            .repeated(RepeatableSpec::infinite()),
    );

    Page(vec![
        Section(
            "Filled meshes",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    "Pre-tessellated triangles drawn via draw_vector_mesh with an affine transform \
                     and per-vertex colors.",
                ),
                Canvas(
                    Modifier::new()
                        .size(560.0, 200.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 16.0)
                        .clip_rounded(16.0),
                    move |ds| {
                        let th = theme();
                        ds.draw_vector_mesh(
                            star_mesh(80.0, 90.0, 36.0, 68.0, 5, th.primary),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Solid,
                        );
                        ds.draw_vector_mesh(
                            circle_mesh(190.0, 90.0, 56.0, 64, th.secondary),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Solid,
                        );
                        ds.draw_vector_mesh(
                            star_mesh(320.0, 90.0, 24.0, 52.0, 6, th.tertiary),
                            rotate_around(Vec2 { x: 320.0, y: 90.0 }, spin),
                            PaintDesc::Solid,
                        );
                        ds.draw_vector_mesh(
                            polygon_mesh(
                                &[
                                    [440.0, 40.0],
                                    [520.0, 60.0],
                                    [500.0, 140.0],
                                    [430.0, 130.0],
                                ],
                                th.error,
                            ),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Solid,
                        );
                        ds.draw_text(
                            "static · static · spinning star · quad",
                            Vec2 { x: 22.0, y: 178.0 },
                            th.on_surface_variant,
                            12.0,
                        );
                    },
                ),
            )),
        ),
        Section(
            "Linear gradient paint",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    "PaintDesc::Linear interpolates between two colors in the mesh's local space; \
                     the fragment shader premultiplies.",
                ),
                Canvas(
                    Modifier::new()
                        .size(560.0, 200.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 16.0)
                        .clip_rounded(16.0),
                    move |ds| {
                        let th = theme();
                        ds.draw_vector_mesh(
                            circle_mesh(110.0, 90.0, 70.0, 64, Color::WHITE),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Linear {
                                start: Vec2 { x: 40.0, y: 20.0 },
                                end: Vec2 { x: 180.0, y: 160.0 },
                                start_color: th.primary,
                                end_color: th.tertiary,
                            },
                        );
                        ds.draw_vector_mesh(
                            star_mesh(300.0, 90.0, 42.0, 74.0, 5, Color::WHITE),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Linear {
                                start: Vec2 { x: 230.0, y: 20.0 },
                                end: Vec2 { x: 370.0, y: 20.0 },
                                start_color: th.secondary,
                                end_color: th.error,
                            },
                        );
                        ds.draw_vector_mesh(
                            polygon_mesh(
                                &[
                                    [420.0, 50.0],
                                    [520.0, 50.0],
                                    [500.0, 160.0],
                                    [440.0, 160.0],
                                ],
                                Color::WHITE,
                            ),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Linear {
                                start: Vec2 { x: 420.0, y: 105.0 },
                                end: Vec2 { x: 520.0, y: 105.0 },
                                start_color: th.error,
                                end_color: th.secondary,
                            },
                        );
                    },
                ),
            )),
        ),
        Section(
            "Vector clip",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    "push_vector_clip stamps the mask into the stencil buffer with IncrementClamp; \
                     mesh content uses an Equal stencil compare so it is clipped to the exact mask \
                     shape, not just its bounding box. Pop decrements back.",
                ),
                Canvas(
                    Modifier::new()
                        .size(560.0, 220.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 16.0)
                        .clip_rounded(16.0),
                    move |ds| {
                        let th = theme();
                        ds.push_vector_clip(star_mesh(200.0, 110.0, 70.0, 120.0, 5, Color::WHITE));
                        for i in 0..6 {
                            let a = i as f32 * std::f32::consts::TAU / 6.0;
                            let (s, c) = a.sin_cos();
                            let cx = 200.0 + c * 55.0;
                            let cy = 110.0 + s * 55.0;
                            let colors = [th.primary, th.secondary, th.tertiary, th.error];
                            ds.draw_vector_mesh(
                                circle_mesh(cx, cy, 34.0, 48, colors[i % colors.len()]),
                                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                PaintDesc::Solid,
                            );
                        }
                        ds.pop_vector_clip();

                        // Unclipped companion outside the mask.
                        ds.draw_vector_mesh(
                            circle_mesh(430.0, 110.0, 40.0, 48, th.surface_container_highest),
                            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            PaintDesc::Solid,
                        );
                        ds.draw_text(
                            "content inside a star clip; one circle sits outside",
                            Vec2 { x: 280.0, y: 198.0 },
                            th.on_surface_variant,
                            12.0,
                        );
                    },
                ),
            )),
        ),
        Section(
            "Screen-space overlay",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    "draw_vector_overlay draws in final device pixels with no world transform — the \
                     right tool for playheads, handles, and rubber-band selectors.",
                ),
                Canvas(
                    Modifier::new()
                        .size(560.0, 120.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 16.0)
                        .clip_rounded(16.0),
                    move |ds| {
                        let th = theme();
                        let t = spin.fract();
                        let x = 40.0 + t * 480.0;
                        let track = |y: f32| {
                            polygon_mesh(
                                &[
                                    [24.0, y - 1.0],
                                    [536.0, y - 1.0],
                                    [536.0, y + 1.0],
                                    [24.0, y + 1.0],
                                ],
                                th.surface_container_highest,
                            )
                        };
                        let handle = |cx: f32, cy: f32| {
                            polygon_mesh(
                                &[
                                    [cx, cy - 10.0],
                                    [cx + 9.0, cy],
                                    [cx, cy + 10.0],
                                    [cx - 9.0, cy],
                                ],
                                th.primary,
                            )
                        };
                        ds.draw_vector_overlay(Arc::new([(*track(50.0)).clone()]));
                        ds.draw_vector_overlay(Arc::new([(*handle(x, 50.0)).clone()]));
                    },
                ),
            )),
        ),
    ])
}
