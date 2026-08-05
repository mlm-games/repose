#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use repose_core::*;
use repose_tree::NodeId;
use rustc_hash::FxHasher;

use crate::Interactions;
use crate::anim::{animate_color, animate_f32};
use crate::textfield::{TF_FONT_DP, TextFieldState, TextMeasureConfig, measure_text};

use super::*;

impl LayoutEngine {
    pub(crate) fn walk_tick(&self, root_id: NodeId) {
        let mut stack = vec![root_id];
        while let Some(id) = stack.pop() {
            let Some(n) = self.tree.get(id) else { continue };
            let scroll_tick = n.modifier.scroll.as_ref().and_then(|s| match s {
                ScrollBinding::Vertical(b) => b.tick.clone(),
                ScrollBinding::Horizontal(b) => b.tick.clone(),
                ScrollBinding::Both(b) => b.tick.clone(),
            });
            if let Some(tick) = scroll_tick {
                tick();
            }
            for &ch in n.children.iter() {
                stack.push(ch);
            }
        }
    }

    pub(crate) fn paint(
        &mut self,
        root_id: NodeId,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        font_px: &dyn Fn(f32) -> f32,
    ) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
        let mut scene = Scene {
            clear_color: locals::theme().background,
            nodes: vec![],
        };
        let mut hits = Vec::new();
        let mut sems = Vec::new();
        let mut deferred: Vec<(NodeId, (f32, f32), f32, Option<u64>, f32)> = Vec::new();
        let mut deferred_blockers: Vec<(f32, repose_core::Rect)> = Vec::new();

        self.walk_paint(
            root_id,
            &mut scene,
            &mut hits,
            &mut sems,
            textfield_states,
            interactions,
            focused,
            (0.0, 0.0),
            1.0,
            None,
            None, // interaction_source
            font_px,
            true,
            &mut deferred,
            false, // Allow deferral in first pass
        );

        // Paint deferred nodes sorted by render_z_index (ascending = higher on top)
        deferred.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(Ordering::Equal));
        for (node_id, parent_offset_px, alpha_accum, sem_parent, z) in deferred.iter().copied() {
            let view_id = *self.view_ids.get(&node_id).unwrap_or(&0);
            let layout = self.layout_for_node(node_id);
            let rect = repose_core::Rect {
                x: parent_offset_px.0 + layout.location.x,
                y: parent_offset_px.1 + layout.location.y,
                w: layout.size.width,
                h: layout.size.height,
            };
            if let Some(node) = self.tree.get(node_id)
                && node.modifier.input_blocker
                && !node.modifier.hit_passthrough
            {
                deferred_blockers.push((z, rect));
            }
            self.walk_paint(
                node_id,
                &mut scene,
                &mut hits,
                &mut sems,
                textfield_states,
                interactions,
                focused,
                parent_offset_px,
                alpha_accum,
                sem_parent,
                None, // interaction_source
                font_px,
                true,
                &mut Vec::new(), // No further deferral in second pass
                true,            // Skip defer check
            );
            let _ = view_id;
        }
        deferred.clear();

        if !deferred_blockers.is_empty() {
            deferred_blockers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            let max_z = hits
                .iter()
                .map(|h| h.z_index)
                .fold(0.0_f32, |a, b| a.max(b));
            let bump = max_z + 1.0;
            for (i, (_z, rect)) in deferred_blockers.iter().enumerate() {
                let blocker_id = u64::MAX - i as u64;
                hits.push(HitRegion {
                    id: blocker_id,
                    rect: *rect,
                    z_index: bump + i as f32,
                    ..Default::default()
                });
            }
        }

        hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));
        (scene, hits, sems)
    }

    pub(crate) fn paint_stamp_hash(
        &self,
        root: NodeId,
        interactions: &Interactions,
        focused: Option<u64>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        sem_parent: Option<u64>,
        alpha_accum: f32,
    ) -> u64 {
        let mut h = FxHasher::default();
        sem_parent.hash(&mut h);
        let alpha_q: u8 = (alpha_accum.clamp(0.0, 1.0) * 255.0).round() as u8;
        alpha_q.hash(&mut h);
        interactions.hover.hash(&mut h);
        focused.hash(&mut h);

        repose_core::animation_driver::live_epoch().hash(&mut h);
        if !interactions.pressed.is_empty() {
            let mut pressed: Vec<u64> = interactions.pressed.iter().copied().collect();
            pressed.sort_unstable();
            pressed.hash(&mut h);
        }

        let mut stack = Vec::new();
        stack.push(root);
        while let Some(id) = stack.pop() {
            let Some(n) = self.tree.get(id) else { continue };
            if let Some(s) = &n.modifier.scroll {
                match s {
                    ScrollBinding::Vertical(b) => {
                        if let Some(get) = &b.get_offset_main {
                            let q = (get() * 8.0) as i32;
                            q.hash(&mut h);
                        }
                    }
                    ScrollBinding::Horizontal(b) => {
                        if let Some(get) = &b.get_offset_main {
                            let q = (get() * 8.0) as i32;
                            q.hash(&mut h);
                        }
                    }
                    ScrollBinding::Both(b) => {
                        if let Some(get) = &b.get_offset_xy {
                            let (x, y) = get();
                            ((x * 8.0) as i32).hash(&mut h);
                            ((y * 8.0) as i32).hash(&mut h);
                        }
                    }
                }
            }
            if n.modifier.text_input.is_some() {
                let vid = *self.view_ids.get(&id).unwrap_or(&0);
                let tf_key = vid;
                if let Some(st_rc) = textfield_states.get(&tf_key) {
                    let st = st_rc.borrow();
                    let mut th = FxHasher::default();
                    st.text.hash(&mut th);
                    th.finish().hash(&mut h);
                    st.selection.start.hash(&mut h);
                    st.selection.end.hash(&mut h);
                    if let Some(r) = &st.composition {
                        r.start.hash(&mut h);
                        r.end.hash(&mut h);
                    } else {
                        0usize.hash(&mut h);
                        0usize.hash(&mut h);
                    }
                    ((st.scroll_offset * 8.0) as i32).hash(&mut h);
                    ((st.scroll_offset_y * 8.0) as i32).hash(&mut h);
                    st.caret_visible().hash(&mut h);
                }
            }
            for &ch in n.children.iter() {
                stack.push(ch);
            }
        }
        h.finish()
    }

    pub(crate) fn find_ancestor_nested_scroll(&self, node_id: NodeId) -> Option<NestedScrollConnection> {
        let mut current = node_id;
        loop {
            let node = self.tree.get(current)?;
            if node.parent.is_none() {
                return None;
            }
            let parent_id = node.parent?;
            let parent = self.tree.get(parent_id)?;
            if let Some(ref conn) = parent.modifier.nested_scroll_connection {
                return Some(conn.clone());
            }
            current = parent_id;
        }
    }

    pub(crate) fn walk_paint(
        &mut self,
        node_id: NodeId,
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        sems: &mut Vec<SemNode>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        parent_offset_px: (f32, f32),
        alpha_accum: f32,
        sem_parent: Option<u64>,
        interaction_source: Option<u64>,
        font_px: &dyn Fn(f32) -> f32,
        allow_cache: bool,
        deferred: &mut Vec<(NodeId, (f32, f32), f32, Option<u64>, f32)>,
        skip_defer: bool,
    ) {
        let (subtree_hash, modifier, kind, children) = {
            let n = self.tree.get(node_id).unwrap();
            (
                n.subtree_hash,
                n.modifier.clone(),
                n.kind.clone(),
                n.children.clone(),
            )
        };

        let view_id = *self.view_ids.get(&node_id).unwrap_or(&0);

        // Check if this node should be deferred for later painting
        if !skip_defer
            && let Some(render_z) = modifier.render_z_index
            && (!deferred.is_empty() || render_z != 0.0)
        {
            // Defer this node - it will be painted later based on render_z_index
            deferred.push((node_id, parent_offset_px, alpha_accum, sem_parent, render_z));
            return;
        }
        debug_assert!(view_id != 0);

        let layout = self.layout_for_node(node_id);

        let local_rect = repose_core::Rect {
            x: layout.location.x,
            y: layout.location.y,
            w: layout.size.width,
            h: layout.size.height,
        };
        let mut rect = repose_core::Rect {
            x: parent_offset_px.0 + local_rect.x,
            y: parent_offset_px.1 + local_rect.y,
            w: local_rect.w,
            h: local_rect.h,
        };

        let mut content_rect = {
            let pl = layout.padding.left;
            let pr = layout.padding.right;
            let pt = layout.padding.top;
            let pb = layout.padding.bottom;
            if pl > 0.0 || pr > 0.0 || pt > 0.0 || pb > 0.0 {
                repose_core::Rect {
                    x: rect.x + pl,
                    y: rect.y + pt,
                    w: (rect.w - pl - pr).max(0.0),
                    h: (rect.h - pt - pb).max(0.0),
                }
            } else {
                rect
            }
        };

        let base_px = (rect.x, rect.y);

        let is_hovered = interactions.hover == Some(view_id);
        let is_pressed = interactions.pressed.contains(&view_id);

        let has_pointer = modifier.on_pointer_down.is_some()
            || modifier.on_pointer_move.is_some()
            || modifier.on_pointer_up.is_some()
            || modifier.on_pointer_enter.is_some()
            || modifier.on_pointer_leave.is_some()
            || modifier.on_double_click.is_some()
            || modifier.on_long_click.is_some();

        let has_dnd = modifier.on_drag_start.is_some()
            || modifier.on_drag_end.is_some()
            || modifier.on_drag_enter.is_some()
            || modifier.on_drag_over.is_some()
            || modifier.on_drag_leave.is_some()
            || modifier.on_drop.is_some();

        let kind_handles_hit = modifier.text_input.is_some()
            || modifier.scroll.is_some()
            || matches!(kind, ViewKind::Expander { .. } | ViewKind::TreeRow { .. });

        let needs_hit = !modifier.disabled
            && (has_pointer
                || modifier.click
                || has_dnd
                || modifier.on_action.is_some()
                || modifier.focusable == Some(true)
                || (modifier.input_blocker && !modifier.hit_passthrough));

        // A node that creates its own hit region is its own interactive surface.
        // Resolve hover/press from its own hit instead of inheriting the state of
        // a clickable ancestor (e.g. a full-screen modal dimmer).
        let owns_hit = needs_hit && !kind_handles_hit && !modifier.hit_passthrough;

        let effective_interaction = interaction_source.unwrap_or(view_id);
        let implicit_hovered = interactions.hover == Some(effective_interaction);
        let implicit_pressed = interactions.pressed.contains(&effective_interaction);
        let (state_hovered, state_pressed) = if let Some(ref src) = modifier.interaction_source {
            (
                src.collect_is_hovered() || is_hovered,
                src.collect_is_pressed() || is_pressed,
            )
        } else if owns_hit {
            (is_hovered, is_pressed)
        } else {
            (implicit_hovered, implicit_pressed)
        };
        let is_focused = focused == Some(view_id);
        let this_alpha = modifier.alpha.unwrap_or(1.0);
        let alpha_accum = (alpha_accum * this_alpha).clamp(0.0, 1.0);
        let alpha_q: u8 = (alpha_accum * 255.0).round() as u8;

        // Repaint Boundary
        if allow_cache && modifier.repaint_boundary {
            let stamp = self.paint_stamp_hash(
                node_id,
                interactions,
                focused,
                textfield_states,
                sem_parent,
                alpha_accum,
            );
            if let Some(entry) = self.paint_cache.get(&node_id)
                && entry.subtree_hash == subtree_hash
                && entry.stamp == stamp
                && entry.rect == rect
                && entry.parent_offset_px == parent_offset_px
                && entry.sem_parent == sem_parent
                && entry.alpha_q == alpha_q
            {
                self.stats.paint_cache_hits += 1;
                scene.nodes.extend(entry.nodes.iter().cloned());
                hits.extend(entry.hits.iter().cloned());
                sems.extend(entry.sems.iter().cloned());
                return;
            }
            self.stats.paint_cache_misses += 1;
            let mut local_scene = Scene {
                clear_color: scene.clear_color,
                nodes: Vec::new(),
            };
            let mut local_hits = Vec::new();
            let mut local_sems = Vec::new();
            self.walk_paint(
                node_id,
                &mut local_scene,
                &mut local_hits,
                &mut local_sems,
                textfield_states,
                interactions,
                focused,
                parent_offset_px,
                alpha_accum / this_alpha.max(1e-6),
                sem_parent,
                interaction_source,
                font_px,
                false,
                &mut Vec::new(), // Don't defer within repaint boundary
                true,            // Skip defer check in repaint boundary
            );

            let entry = PaintCacheEntry {
                subtree_hash,
                stamp,
                rect,
                parent_offset_px,
                sem_parent,
                alpha_q,
                nodes: Arc::new(local_scene.nodes.clone()),
                hits: Arc::new(local_hits.clone()),
                sems: Arc::new(local_sems.clone()),
            };
            self.paint_cache.insert(node_id, entry);
            scene.nodes.extend(local_scene.nodes);
            hits.extend(local_hits);
            sems.extend(local_sems);
            return;
        }

        let round_clip_px = clamp_radii(
            modifier
                .clip_rounded
                .map(|r| r.map(dp_to_px))
                .unwrap_or([0.0; 4]),
            rect.w,
            rect.h,
        );
        let push_round_clip =
            round_clip_px.iter().any(|&r| r > 0.5) && rect.w > 0.5 && rect.h > 0.5;

        if let Some(anim_spec) = &modifier.animate_content_size {
            let target = repose_core::Size {
                width: rect.w,
                height: rect.h,
            };

            let anim_key = format!("anim_cs:{view_id}");
            let anim =
                remember_state_with_key(&anim_key, || AnimatedValue::new(target, *anim_spec));
            let last_target = remember_state_with_key(format!("anim_cs_last:{view_id}"), || {
                repose_core::Size::default()
            });

            // Check if target changed, and re-start animation from current value
            let mut lt = last_target.borrow_mut();
            if (lt.width - target.width).abs() > 0.5 || (lt.height - target.height).abs() > 0.5 {
                *lt = target;
                drop(lt);
                let mut a = anim.borrow_mut();
                a.set_spec(*anim_spec);
                a.set_target(target);

                // Register with AnimationDriver for pre-composition advancement
                let reg_key = anim_key;
                let reg_anim = anim.clone();
                repose_core::animation_driver::register(
                    reg_key,
                    std::rc::Rc::new(std::cell::RefCell::new(move || {
                        reg_anim.borrow_mut().update()
                    })),
                );
                request_frame();
            } else {
                drop(lt);
            }

            let s = anim.borrow().get().clone();
            let animated = repose_core::Size {
                width: s.width.max(1.0),
                height: s.height.max(1.0),
            };

            // Override rect and content_rect dimensions with animated values
            let dw = rect.w - animated.width;
            let dh = rect.h - animated.height;
            rect.w = animated.width;
            rect.h = animated.height;
            content_rect.w = (content_rect.w - dw).max(0.0);
            content_rect.h = (content_rect.h - dh).max(0.0);
        }

        if let Some(tf) = modifier.transform {
            let mut adjusted = tf;
            let pivot_x = rect.x + rect.w * tf.origin_x;
            let pivot_y = rect.y + rect.h * tf.origin_y;
            let cos_a = tf.rotate.cos();
            let sin_a = tf.rotate.sin();
            let sp_x = pivot_x * tf.scale_x;
            let sp_y = pivot_y * tf.scale_y;
            adjusted.translate_x += pivot_x - (sp_x * cos_a - sp_y * sin_a);
            adjusted.translate_y += pivot_y - (sp_x * sin_a + sp_y * cos_a);
            adjusted.origin_x = 0.0;
            adjusted.origin_y = 0.0;
            scene.nodes.push(SceneNode::PushTransform { transform: adjusted });
        }
        let overflow_clip = modifier
            .overflow
            .map_or(true, |o| o == repose_core::Overflow::Clip);
        if push_round_clip && overflow_clip {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: round_clip_px,
                op: ClipOp::Intersect,
            });
        }
        if let Some(cr) = modifier.clip_rect
            && overflow_clip
        {
            scene.nodes.push(SceneNode::PushClip {
                rect: repose_core::Rect {
                    x: rect.x + dp_to_px(cr.left),
                    y: rect.y + dp_to_px(cr.top),
                    w: (dp_to_px(cr.right) - dp_to_px(cr.left)).max(0.0),
                    h: (dp_to_px(cr.bottom) - dp_to_px(cr.top)).max(0.0),
                },
                radius: [0.0; 4],
                op: cr.op,
            });
        }
        let push_bounds_clip = overflow_clip
            && (matches!(
                modifier.overflow,
                Some(repose_core::Overflow::Clip)
            ) || modifier.animate_content_size.is_some())
            && !push_round_clip
            && modifier.clip_rect.is_none()
            && (rect.w > 0.0 || rect.h > 0.0);
        if push_bounds_clip {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: [0.0; 4],
                op: ClipOp::Intersect,
            });
        }

        // rendered behind the component
        if let Some(se) = &modifier.state_elevation {
            let target = if modifier.disabled {
                se.disabled
            } else if state_pressed {
                se.pressed
            } else if state_hovered {
                se.hovered
            } else {
                se.default
            };
            let elev = animate_f32(
                format!("m3_elev:{view_id}"),
                target,
                locals::theme().motion.shape,
            );
            if elev > 0.5 {
                let shadow_offset = elev * 0.5;
                let shadow_alpha = ((elev / 24.0).clamp(0.0, 1.0) * 0.25 * 255.0) as u8;
                scene.nodes.push(SceneNode::Shadow {
                    rect: repose_core::Rect {
                        x: rect.x + shadow_offset * 0.5,
                        y: rect.y + shadow_offset,
                        w: rect.w,
                        h: rect.h,
                    },
                    radius: round_clip_px,
                    elevation: elev,
                    color: Color(0, 0, 0, shadow_alpha),
                });
            }
        }

        // Draw background (always)
        if let Some(bg) = modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                brush: mul_alpha_brush(bg, alpha_accum),
                radius: round_clip_px,
            });
        }
        // State layer as overlay on top (independently animated alpha)
        if let Some(sc) = &modifier.state_colors {
            let target = if modifier.disabled {
                sc.disabled
            } else if state_pressed {
                sc.pressed
            } else if state_hovered {
                sc.hovered
            } else {
                sc.default
            };
            let overlay = animate_color(
                format!("m3_sc:{view_id}"),
                target,
                locals::theme().motion.color,
            );
            if overlay.3 > 0 {
                scene.nodes.push(SceneNode::Rect {
                    rect,
                    brush: mul_alpha_brush(Brush::Solid(overlay), alpha_accum),
                    radius: round_clip_px,
                });
            }
        }

        if let Some(b) = &modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: mul_alpha_color(b.color, alpha_accum),
                width: dp_to_px(b.width),
                radius: clamp_radii(
                    max_radii(
                        b.radius.map(dp_to_px),
                        modifier
                            .clip_rounded
                            .map(|r| r.map(dp_to_px))
                            .unwrap_or([0.0; 4]),
                    ),
                    rect.w,
                    rect.h,
                ),
            });
        }
        // Native text field painting (Compose-aligned: text_input modifier triggers built-in paint)
        if let Some(ref ti) = modifier.text_input {
            let tf_key = view_id;
            let state = textfield_states.get(&tf_key).cloned();
            let is_focused = focused == Some(view_id);

            if let Some(ref state_rc) = state {
                let mut st = state_rc.borrow_mut();
                st.set_inner_width(content_rect.w);
                st.set_inner_height(content_rect.h);
                st.tick_scroll_animation();
                if let Some(ref vt) = ti.visual_transformation.as_ref() {
                    let empty = repose_core::AnnotatedString::new(String::new(), vec![]);
                    let tfmd = vt.filter(&empty);
                    st.offset_map = Some(tfmd.offset_mapping.clone_box());
                    st.visual_transformation = Some((*vt).clone());
                } else {
                    st.offset_map = None;
                    st.visual_transformation = None;
                }
                drop(st);
            }

            crate::textfield::paint_text_field(
                scene,
                content_rect,
                ti,
                state.as_ref(),
                is_focused,
                modifier.clip_rounded,
                alpha_accum,
            );
        }
        if let Some(p) = &modifier.painter {
            (p)(scene, rect, alpha_accum);
        }

        // Draw indication (ripple, etc.) on top of content but behind hit regions.
        let indication_factory = modifier.indication.clone().or_else(local_indication);
        let indication_source = if let Some(ref src) = modifier.interaction_source {
            Some(src.clone())
        } else if indication_factory.is_some() && owns_hit {
            let msrc = remember_state_with_key(
                format!("rx:ixsrc:{view_id}"),
                MutableInteractionSource::new,
            );
            Some(msrc.borrow().source())
        } else {
            None
        };
        if let (Some(ref factory), Some(ref interaction_source)) =
            (indication_factory.as_ref(), indication_source.as_ref())
        {
            let draw_node = factory.create(interaction_source);
            draw_node.draw(scene, rect, alpha_accum);
        }

        if owns_hit {
            let focusable = modifier.focusable.unwrap_or(true);
            let mut hit = HitRegion {
                id: view_id,
                rect,
                z_index: modifier.z_index,
                focusable,
                focus_group_id: if modifier.focus_group {
                    Some(view_id)
                } else {
                    self.focus_group_stack.last().copied()
                },
                ..HitRegion::from_modifier(view_id, rect, &modifier)
            };

            // Auto-wire InteractionSource to pointer/hover callbacks.
            // The source's state is OR'd with the implicit view-ID state in state resolution above.
            if let Some(ref src) = indication_source {
                let msrc = src.to_mutable();
                let last_press_id: Rc<Cell<Option<PressId>>> = Rc::new(Cell::new(None));

                // Wrap on_pointer_down to emit Press with position + unique ID.
                // Pointer events arrive in window coordinates; the ripple origin
                // is computed relative to the view rect, so convert to local.
                let orig_down = hit.on_pointer_down.take();
                let s_down = msrc.clone();
                let lpid_down = last_press_id.clone();
                let rect_origin = (rect.x, rect.y);
                hit.on_pointer_down = Some(Rc::new(move |ev| {
                    let local = Vec2 {
                        x: ev.position.x - rect_origin.0,
                        y: ev.position.y - rect_origin.1,
                    };
                    let press = Interaction::new_press(local);
                    if let Interaction::Press(id, _) = press {
                        lpid_down.set(Some(id));
                    }
                    s_down.emit(press);
                    if let Some(ref f) = orig_down {
                        f(ev);
                    }
                }));

                // Wrap on_pointer_up to emit Release with the last press ID.
                // Always emit Release even when the Cell is empty (composition can
                // change between press and release, creating a fresh Cell per frame).
                let orig_up = hit.on_pointer_up.take();
                let s_up = msrc.clone();
                let lpid_up = last_press_id.clone();
                hit.on_pointer_up = Some(Rc::new(move |ev| {
                    let pid = lpid_up.take().unwrap_or(0);
                    s_up.emit(Interaction::Release(pid));
                    if let Some(ref f) = orig_up {
                        f(ev);
                    }
                }));

                // Wrap on_pointer_cancel to emit Cancel with the last press ID.
                let orig_cancel = hit.on_pointer_cancel.take();
                let s_cancel = msrc.clone();
                hit.on_pointer_cancel = Some(Rc::new(move |ev| {
                    let pid = last_press_id.take().unwrap_or(0);
                    s_cancel.emit(Interaction::Cancel(pid));
                    if let Some(ref f) = orig_cancel {
                        f(ev);
                    }
                }));

                let orig_enter = hit.on_pointer_enter.take();
                let s_enter = msrc.clone();
                hit.on_pointer_enter = Some(Rc::new(move |ev| {
                    s_enter.emit(Interaction::HoverEnter);
                    if let Some(ref f) = orig_enter {
                        f(ev);
                    }
                }));

                let orig_leave = hit.on_pointer_leave.take();
                hit.on_pointer_leave = Some(Rc::new(move |ev| {
                    msrc.emit(Interaction::HoverLeave);
                    if let Some(ref f) = orig_leave {
                        f(ev);
                    }
                }));
            }

            hits.push(hit);
        }

        // Focus ring for interactive views
        if is_focused
            && (has_pointer
                || modifier.click
                || modifier.on_action.is_some()
                || modifier.focusable == Some(true))
        {
            push_focus_ring(scene, rect, focus_radius(&modifier));
        }

        let child_interaction_source = if owns_hit {
            Some(view_id)
        } else {
            interaction_source
        };

        let mut next_sem_parent = sem_parent;

        // Push focus group scope for child traversal
        if modifier.focus_group {
            self.focus_group_stack.push(view_id);
        }

        match &kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                overflow,
                font_family,
                annotations,
                text_align,
                font_weight,
                font_style,
                text_decoration,
                letter_spacing,
                line_height,
                url,
                font_variation_settings,
                ..
            } => {
                let tl = self.text_cache.get(&node_id);
                let (size_px, line_h_px, lines, line_ranges) = if let Some(tl) = tl {
                    (
                        tl.size_px,
                        tl.line_h_px,
                        tl.lines.clone(),
                        Some(tl.line_ranges.clone()),
                    )
                } else {
                    let px = font_px(*font_size);
                    let lh = if *line_height > 0.0 {
                        font_px(*line_height)
                    } else {
                        px
                    };
                    (px, lh, vec![text.clone()], None)
                };
                let total_h = lines.len() as f32 * line_h_px;
                let need_v_clip =
                    total_h > content_rect.h + 0.5 && *overflow != TextOverflow::Visible;

                let need_clip =
                    *overflow != TextOverflow::Visible && (need_v_clip || content_rect.w > 0.0);
                if need_clip {
                    scene.nodes.push(SceneNode::PushClip {
                        rect: content_rect,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });
                }

                let has_annotations = annotations.as_ref().map(|a| !a.is_empty()).unwrap_or(false);

                if has_annotations {
                    // Emit one SceneNode::Text per styled segment per line
                    let annos = annotations.as_ref().unwrap();
                    for (i, ln) in lines.iter().enumerate() {
                        let line_start = line_ranges
                            .as_ref()
                            .and_then(|r| r.get(i).map(|&(s, _)| s))
                            .unwrap_or(0);
                        let line_end = line_ranges
                            .as_ref()
                            .and_then(|r| r.get(i).map(|&(_, e)| e))
                            .unwrap_or(ln.len());

                        struct SegInfo {
                            start: usize,
                            end: usize,
                            color: Color,
                            font_dp: f32,
                            decoration: TextDecoration,
                            url: Option<Arc<str>>,
                            font_weight: u16,
                            font_family: Option<&'static str>,
                            font_style: u8,
                            letter_spacing: f32,
                            line_height: f32,
                            background: Option<Color>,
                            alpha: f32,
                            text_direction: TextDirection,
                            font_synthesis: FontSynthesis,
                            baseline_shift: BaselineShift,
                            hyphens: Hyphens,
                            line_break: LineBreak,
                            text_indent: Option<TextIndent>,
                            draw_style: DrawStyle,
                            w: f32,
                            px: f32,
                            font_variation_settings: Option<Arc<str>>,
                        }
                        fn style_to_fs(style: &FontStyle) -> u8 {
                            if matches!(style, FontStyle::Italic) { 1 } else { 0 }
                        }
                        // Build segments from spans
                        let mut segments: Vec<SegInfo> = Vec::new();
                        let mut cursor = line_start;

                        let mut relevant: Vec<&TextSpan> = annos
                            .iter()
                            .filter(|s| s.start < line_end && s.end > line_start)
                            .collect();
                        relevant.sort_by_key(|s| s.start);

                        for span in &relevant {
                            let seg_start = span.start.max(line_start);
                            let seg_end = span.end.min(line_end);

                            if seg_start > cursor {
                                // Default-styled segment before this span
                                segments.push(SegInfo {
                                    start: cursor,
                                    end: seg_start,
                                    color: *color,
                                    font_dp: *font_size,
                                    decoration: *text_decoration,
                                    url: None,
                                    font_weight: font_weight.0,
                                    font_family: *font_family,
                                    font_style: style_to_fs(font_style),
                                    letter_spacing: *letter_spacing,
                                    line_height: *line_height,
                                    background: None,
                                    alpha: 0.0,
                                    text_direction: text_direction(),
                                    font_synthesis: FontSynthesis::Unspecified,
                                    baseline_shift: BaselineShift::Unspecified,
                                    hyphens: Hyphens::Unspecified,
                                    line_break: LineBreak::Unspecified,
                                    text_indent: None,
                                    draw_style: DrawStyle::Fill,
                                    w: 0.0,
                                    px: 0.0,
                                    font_variation_settings: None,
                                });
                            }

                            let span_color = span.style.color.unwrap_or(*color);
                            let span_size = span.style.font_size.unwrap_or(*font_size);
                            let span_decoration = span.style.text_decoration.unwrap_or(*text_decoration);
                            let span_weight = span.style.font_weight.unwrap_or(font_weight.0);
                            let span_family = span.style.font_family.or(*font_family);
                            let span_style = span.style.font_style.unwrap_or(style_to_fs(font_style));
                            let span_ls = span.style.letter_spacing.unwrap_or(*letter_spacing);
                            let span_lh = span.style.line_height.unwrap_or(*line_height);
                            let span_bg = span.style.background;
                            let span_alpha = span.style.alpha;
                            let span_td = span.style.text_direction.unwrap_or(text_direction());
                            let span_fs = span.style.font_synthesis.unwrap_or(FontSynthesis::Unspecified);
                            let span_bs = span.style.baseline_shift.unwrap_or(BaselineShift::Unspecified);
                            let span_h = span.style.hyphens.unwrap_or(Hyphens::Unspecified);
                            let span_lb = span.style.line_break.unwrap_or(LineBreak::Unspecified);
                            let span_ti = span.style.text_indent;
                            let span_ds = span.style.draw_style.clone().unwrap_or(DrawStyle::Fill);
                            let span_url = span.url.clone();
                            let span_fvs = span.style.font_variation_settings.clone().map(Arc::from);
                            segments.push(SegInfo {
                                start: seg_start,
                                end: seg_end,
                                color: span_color,
                                font_dp: span_size,
                                decoration: span_decoration,
                                url: span_url,
                                font_weight: span_weight,
                                font_family: span_family,
                                font_style: span_style,
                                letter_spacing: span_ls,
                                line_height: span_lh,
                                background: span_bg,
                                alpha: span_alpha,
                                text_direction: span_td,
                                font_synthesis: span_fs,
                                baseline_shift: span_bs,
                                hyphens: span_h,
                                line_break: span_lb,
                                text_indent: span_ti,
                                draw_style: span_ds,
                                w: 0.0,
                                px: 0.0,
                                font_variation_settings: span_fvs,
                            });
                            cursor = seg_end;
                        }

                        if cursor < line_end {
                            segments.push(SegInfo {
                                start: cursor,
                                end: line_end,
                                color: *color,
                                font_dp: *font_size,
                                decoration: *text_decoration,
                                url: None,
                                font_weight: font_weight.0,
                                font_family: *font_family,
                                font_style: style_to_fs(font_style),
                                letter_spacing: *letter_spacing,
                                line_height: *line_height,
                                background: None,
                                alpha: 0.0,
                                text_direction: text_direction(),
                                font_synthesis: FontSynthesis::Unspecified,
                                baseline_shift: BaselineShift::Unspecified,
                                hyphens: Hyphens::Unspecified,
                                line_break: LineBreak::Unspecified,
                                text_indent: None,
                                draw_style: DrawStyle::Fill,
                                w: 0.0,
                                px: 0.0,
                                font_variation_settings: None,
                            });
                        }

                        // Measure and emit each segment
                        let seg_font_px =
                            |dp: f32| dp_to_px(dp) * repose_core::locals::text_scale().0;
                        let mut total_w = 0.0f32;
                        let mut seg_measurements: Vec<&mut SegInfo> = segments.iter_mut().collect();
                        for info in &mut seg_measurements {
                            let seg_text = &text[info.start..info.end];
                            if seg_text.is_empty() {
                                continue;
                            }
                            info.px = seg_font_px(info.font_dp);
                            info.w =
                                measure_text(seg_text, info.px, TextMeasureConfig { font_family: info.font_family, font_weight: info.font_weight, font_style: info.font_style, letter_spacing: info.letter_spacing, ..Default::default() })
                                    .positions
                                    .last()
                                    .copied()
                                    .unwrap_or(0.0);
                            total_w += info.w;
                        }
                        let align_x_offset: f32 = match text_align {
                            TextAlign::End | TextAlign::Right => {
                                (content_rect.w - total_w).max(0.0)
                            }
                            TextAlign::Center => {
                                (content_rect.w - total_w).max(0.0) * 0.5
                            }
                            _ => 0.0,
                        };
                        let mut seg_x = content_rect.x + align_x_offset;
                        for info in &segments {
                            let seg_text = &text[info.start..info.end];
                            if seg_text.is_empty() {
                                continue;
                            }
                            let seg_rect = repose_core::Rect {
                                x: seg_x,
                                y: content_rect.y + i as f32 * line_h_px,
                                w: info.w,
                                h: line_h_px,
                            };
                            let seg_color = if info.alpha > 0.0 {
                                mul_alpha_color(info.color, info.alpha)
                            } else {
                                info.color
                            };
                            if let Some(bg) = &info.background {
                                let bg_rect = repose_core::Rect {
                                    x: seg_x,
                                    y: seg_rect.y,
                                    w: info.w,
                                    h: line_h_px,
                                };
                                scene.nodes.push(SceneNode::Rect {
                                    rect: bg_rect,
                                    brush: Brush::Solid(mul_alpha_color(*bg, alpha_accum)),
                                    radius: [0.0; 4],
                                });
                            }
                            scene.nodes.push(SceneNode::Text {
                                rect: seg_rect,
                                text: Arc::<str>::from(seg_text.to_string().into_boxed_str()),
                                color: mul_alpha_color(seg_color, alpha_accum),
                                size: info.px,
                                font_family: info.font_family,
                                text_align: *text_align,
                                font_weight: FontWeight(info.font_weight),
                                font_style: if info.font_style == 1 { FontStyle::Italic } else { FontStyle::Normal },
                                text_decoration: info.decoration,
                                letter_spacing: info.letter_spacing,
                                line_height: info.line_height,
                                extra_style: TextExtraStyle {
                                    text_direction: info.text_direction,
                                    font_synthesis: info.font_synthesis,
                                    baseline_shift: info.baseline_shift,
                                    draw_style: info.draw_style.clone(),
                                },
                                url: info.url.clone(),
                                font_variation_settings: info.font_variation_settings.clone(),
                            });
                            // Create hit region for clickable links
                            if let Some(url) = &info.url {
                                let link_id =
                                    view_id ^ ((info.start as u64) << 32) | (info.end as u64);
                                let link_url = url.clone();
                                hits.push(HitRegion {
                                    id: link_id,
                                    rect: seg_rect,
                                    cursor: Some(CursorIcon::Pointer),
                                    on_click: Some(Rc::new(move || {
                                        open_url(&link_url);
                                    })),
                                    ..Default::default()
                                });
                            }
                            seg_x += info.w;
                        }
                    }
                } else {
                    let fw_val = font_weight.0;
                    let fs_val = if matches!(font_style, FontStyle::Italic) {
                        1
                    } else {
                        0
                    };
                    for (i, ln) in lines.iter().enumerate() {
                        let line_w = measure_text(ln, size_px, TextMeasureConfig { font_family: *font_family, font_weight: fw_val, font_style: fs_val, letter_spacing: *letter_spacing, ..Default::default() })
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0);
                        let align_x = |line_w: f32| -> f32 {
                            match text_align {
                                TextAlign::End | TextAlign::Right => {
                                    content_rect.x + (content_rect.w - line_w).max(0.0)
                                }
                                TextAlign::Center => {
                                    content_rect.x + (content_rect.w - line_w).max(0.0) * 0.5
                                }
                                _ => content_rect.x,
                            }
                        };
                        let seg_rect = repose_core::Rect {
                            x: align_x(line_w),
                            y: content_rect.y + i as f32 * line_h_px,
                            w: content_rect.w,
                            h: line_h_px,
                        };
                        scene.nodes.push(SceneNode::Text {
                            rect: seg_rect,
                            text: Arc::<str>::from(ln.clone()),
                            color: mul_alpha_color(*color, alpha_accum),
                            size: size_px,
                            font_family: *font_family,
                            text_align: *text_align,
                            font_weight: *font_weight,
                            font_style: *font_style,
                            text_decoration: *text_decoration,
                            letter_spacing: *letter_spacing,
                            line_height: *line_height,
                            extra_style: Default::default(),
                            url: url.clone(),
                            font_variation_settings: font_variation_settings.clone(),
                        });
                        // Create hit region for view-level URL
                        if let Some(link_url) = url {
                            let link_id = view_id ^ 0x8000_0000_0000_0000;
                            let lu = link_url.clone();
                            hits.push(HitRegion {
                                id: link_id,
                                rect: seg_rect,
                                cursor: Some(CursorIcon::Pointer),
                                on_click: Some(Rc::new(move || {
                                    open_url(&lu);
                                })),
                                ..Default::default()
                            });
                        }
                    }
                }

                if need_clip {
                    scene.nodes.push(SceneNode::PopClip);
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Text,
                    label: Some(text.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Image { handle, tint, fit } => {
                scene.nodes.push(SceneNode::Image {
                    rect,
                    handle: *handle,
                    tint: mul_alpha_color(*tint, alpha_accum),
                    fit: *fit,
                });
            }
            ViewKind::Box if modifier.text_input.is_some() => {
                let ti = modifier.text_input.as_ref().unwrap();
                let multiline = ti.multiline;
                let hint = &ti.hint;
                let on_change = &ti.on_change;
                let on_submit = &ti.on_submit;
                let focus_tracker = &ti.focus_tracker;
                let value = &ti.value;
                let tf_key = view_id;

                if let Some(cell) = focus_tracker.as_ref() {
                    cell.set(is_focused);
                }

                // Sync the controlled value into the TextFieldState
                if let Some(state_rc) = textfield_states.get(&tf_key) {
                    crate::textfield::set_textfield_state(tf_key, state_rc.clone());
                    let mut st = state_rc.borrow_mut();
                    if st.text != *value {
                        st.text = value.clone();
                        st.composition = None;
                        st.drag_anchor = None;
                        let len = st.text.len();
                        let ns = st.selection.start.min(len);
                        let ne = st.selection.end.min(len);
                        if ns != st.selection.start || ne != st.selection.end {
                            st.selection = ns..ne;
                        }
                    }
                }

                // Scroll wheel support for multiline text areas
                let on_scroll = if multiline {
                    let key = tf_key;
                    let h = rect.h;
                    let font_val = font_px(TF_FONT_DP);
                    let wrap_w = rect.w.max(1.0);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_height(h);
                        let layout =
                            crate::textfield::layout_text_area(&st.text, font_val, wrap_w, 400, 0, 0.0, None);
                        let content_h = layout.ranges.len().max(1) as f32 * layout.line_h_px;
                        let max_y = (content_h - st.inner_height).max(0.0);

                        let before = st.scroll_target_y;
                        let target = (st.scroll_target_y + d.y).clamp(0.0, max_y);
                        st.scroll_target_y = target;

                        let consumed = target - before;
                        Vec2 {
                            x: d.x,
                            y: d.y - consumed,
                        }
                    }) as Rc<dyn Fn(Vec2) -> Vec2>)
                } else {
                    // Single-line horizontal scroll (mouse wheel or trackpad)
                    let key = tf_key;
                    let inner_w = rect.w.max(1.0);
                    let font_val = font_px(TF_FONT_DP);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_width(inner_w);
                        let m = crate::textfield::measure_text(&st.text, font_val, TextMeasureConfig::default());
                        let content_w = m.positions.last().copied().unwrap_or(0.0);
                        let max_x = (content_w - st.inner_width).max(0.0);

                        let before = st.scroll_target;
                        let target = (st.scroll_target - d.y).clamp(0.0, max_x);
                        st.scroll_target = target;

                        let consumed = before - target;
                        Vec2 {
                            x: d.x,
                            y: d.y - consumed,
                        }
                    }) as Rc<dyn Fn(Vec2) -> Vec2>)
                };

                if !modifier.hit_passthrough {
                    let user_on_action = modifier.on_action.clone();
                    let change_cb = on_change.clone();
                    let is_multiline = multiline;
                    let tf_on_action: Option<Rc<dyn Fn(repose_core::shortcuts::Action) -> bool>> =
                        Some(Rc::new(move |action| {
                            use repose_core::shortcuts::Action;
                            let Some(st) = crate::textfield::get_textfield_state(tf_key) else {
                                return false;
                            };
                            let mut s = st.borrow_mut();
                            let mut handled = false;
                            match action {
                                Action::Copy => {
                                    let txt = s.selected_text();
                                    if !txt.is_empty() {
                                        repose_core::clipboard::copy_to_clipboard(&txt);
                                        handled = true;
                                    }
                                }
                                Action::Cut => {
                                    let txt = s.selected_text();
                                    if !txt.is_empty() {
                                        repose_core::clipboard::copy_to_clipboard(&txt);
                                        s.insert_text_atomic("");
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                Action::Paste => {
                                    let Some(mut txt) = repose_core::clipboard::paste_text() else {
                                        return false;
                                    };
                                    if is_multiline {
                                        txt.retain(|c| c == '\n' || (!c.is_control() && c != '\r'));
                                    } else {
                                        txt.retain(|c| !c.is_control() && c != '\n' && c != '\r');
                                    }
                                    if txt.is_empty() {
                                        return false;
                                    }
                                    s.insert_text_atomic(&txt);
                                    crate::textfield::ensure_caret_visible(&mut s, is_multiline);
                                    let text = s.text.clone();
                                    drop(s);
                                    if let Some(cb) = &change_cb {
                                        cb(text);
                                    }
                                    handled = true;
                                }
                                Action::SelectAll => {
                                    s.selection = 0..s.text.len();
                                    crate::textfield::ensure_caret_visible(&mut s, is_multiline);
                                    if !s.text.is_empty() {
                                        repose_core::clipboard::set_primary_selection(&s.text);
                                    }
                                    handled = true;
                                }
                                Action::Undo => {
                                    if s.can_undo() {
                                        s.undo();
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                Action::Redo => {
                                    if s.can_redo() {
                                        s.redo();
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                _ => {}
                            }
                            handled
                        }));

                    let combined: Option<Rc<dyn Fn(repose_core::shortcuts::Action) -> bool>> =
                        match (user_on_action, tf_on_action) {
                            (Some(u), Some(t)) => Some(Rc::new(move |a| u(a.clone()) || t(a))),
                            (Some(u), None) => Some(u),
                            (None, Some(t)) => Some(t),
                            (None, None) => None,
                        };

                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll,
                        focusable: true,
                        z_index: modifier.z_index,
                        on_text_change: on_change.clone(),
                        on_text_submit: on_submit.clone(),
                        tf_state_key: Some(tf_key),
                        tf_multiline: multiline,
                        tf_content_origin: Some((content_rect.x, content_rect.y)),
                        on_action: combined,
                        cursor: Some(crate::CursorIcon::Text),
                        focus_group_id: if modifier.focus_group {
                            Some(view_id)
                        } else {
                            self.focus_group_stack.last().copied()
                        },
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }

                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::TextField,
                    label: Some(hint.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Expander { on_toggle, .. } => {
                if let Some(cb) = on_toggle.clone() {
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_click: Some(cb),
                        focusable: true,
                        z_index: modifier.z_index,
                        focus_group_id: if modifier.focus_group {
                            Some(view_id)
                        } else {
                            self.focus_group_stack.last().copied()
                        },
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Button,
                    label: infer_label(&self.tree, node_id),
                    rect,
                    focused: is_focused,
                    enabled: !modifier.disabled,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::TreeRow {
                depth,
                has_children,
                is_expanded,
                is_selected,
                on_toggle,
                on_select,
            } => {
                let indent_px = dp_to_px(*depth as f32 * 16.0);
                let chevron_w = if *has_children { dp_to_px(16.0) } else { 0.0 };

                // Selection highlight
                if *is_selected {
                    let th = locals::theme();
                    scene.nodes.push(SceneNode::Rect {
                        rect,
                        brush: Brush::Solid(th.primary.with_alpha_f32(0.15)),
                        radius: [0.0; 4],
                    });
                }

                // Expand/collapse chevron
                if *has_children {
                    let chevron_text = if *is_expanded { "▼" } else { "▶" };
                    let chevron_px = dp_to_px(12.0);
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: rect.x + indent_px,
                            y: rect.y + (rect.h - chevron_px) * 0.5,
                            w: chevron_w,
                            h: chevron_px,
                        },
                        text: Arc::from(chevron_text),
                        color: mul_alpha_color(locals::theme().on_surface, alpha_accum),
                        size: chevron_px,
                        font_family: None,
                        text_align: TextAlign::Start,
                        font_weight: FontWeight::NORMAL,
                        font_style: FontStyle::Normal,
                        text_decoration: TextDecoration::default(),
                        letter_spacing: 0.0,
                        line_height: 0.0,
                        extra_style: Default::default(),
                        url: None,
                        font_variation_settings: None,
                    });

                    // Toggle hit region (chevron area)
                    if let Some(cb) = on_toggle.clone() {
                        hits.push(HitRegion {
                            id: view_id.wrapping_mul(2).wrapping_add(1),
                            rect: repose_core::Rect {
                                x: rect.x + indent_px,
                                y: rect.y,
                                w: chevron_w,
                                h: rect.h,
                            },
                            on_click: Some(cb),
                            focusable: false,
                            z_index: modifier.z_index,
                            ..HitRegion::from_modifier(view_id, rect, &modifier)
                        });
                    }
                }

                // Select hit region (label area)
                if let Some(cb) = on_select.clone() {
                    hits.push(HitRegion {
                        id: view_id,
                        rect: repose_core::Rect {
                            x: rect.x + indent_px + chevron_w,
                            y: rect.y,
                            w: (rect.w - indent_px - chevron_w).max(0.0),
                            h: rect.h,
                        },
                        on_click: Some(cb),
                        focusable: true,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }

                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Button,
                    label: infer_label(&self.tree, node_id),
                    rect,
                    focused: is_focused,
                    enabled: !modifier.disabled,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            _ => {
                if let Some(s) = &modifier.semantics {
                    sems.push(SemNode {
                        id: view_id,
                        parent: sem_parent,
                        role: s.role,
                        label: s.label.clone(),
                        rect,
                        focused: is_focused,
                        enabled: !modifier.disabled,
                        selectable_group: s.selectable_group,
                    });
                    next_sem_parent = Some(view_id);
                }
            }
        }

        // Fire on_globally_positioned / on_size_changed if position/size changed
        let view_id_for_pos = view_id;
        let dp_rect = repose_core::Rect {
            x: px_to_dp(rect.x),
            y: px_to_dp(rect.y),
            w: px_to_dp(rect.w),
            h: px_to_dp(rect.h),
        };
        if let Some(cb) = &modifier.on_globally_positioned {
            let prev = self.prev_observed_rects.get(&view_id_for_pos).copied();
            if prev != Some(dp_rect) {
                cb(dp_rect);
            }
        }
        if let Some(cb) = &modifier.on_size_changed {
            let prev = self.prev_observed_rects.get(&view_id_for_pos).copied();
            if prev.map(|r| (r.w, r.h)) != Some((dp_rect.w, dp_rect.h)) {
                cb(Vec2 {
                    x: dp_rect.w,
                    y: dp_rect.h,
                });
            }
        }
        if modifier.on_globally_positioned.is_some() || modifier.on_size_changed.is_some() {
            self.prev_observed_rects.insert(view_id_for_pos, dp_rect);
        }

        // Children
        let child_offset_px = base_px;
        let has_blur = modifier
            .blur
            .map_or(false, |b| b.radius_x > 0.0 || b.radius_y > 0.0);
        let layer_id = if modifier.graphics_layer.is_some() || has_blur {
            let id = self.layer_id_counter;
            self.layer_id_counter = self.layer_id_counter.wrapping_add(1);
            let blur_style = modifier.blur.unwrap_or(BlurStyle {
                radius_x: 0.0,
                radius_y: 0.0,
                edge_treatment: BlurredEdgeTreatment::Rectangle,
            });
            let blur_radius_x = dp_to_px(blur_style.radius_x);
            let blur_radius_y = dp_to_px(blur_style.radius_y);
            let alpha = modifier.graphics_layer.unwrap_or(1.0);
            // Snap the layer rect to whole pixels so the composite quad exactly
            // matches the offscreen texture (avoids fractional 1:1 sampling blur
            // on text/graphics inside the layer). The content transform below
            // must use the same snapped origin, or content shifts by <1px.
            let layer_rect = repose_core::Rect {
                x: rect.x.round(),
                y: rect.y.round(),
                w: rect.w.round().max(1.0),
                h: rect.h.round().max(1.0),
            };
            scene.nodes.push(SceneNode::BeginLayer {
                rect: layer_rect,
                layer_id: id,
                alpha,
                blur_radius_x,
                blur_radius_y,
                rectangle_edge: matches!(
                    blur_style.edge_treatment,
                    BlurredEdgeTreatment::Rectangle
                ),
            });
            scene.nodes.push(SceneNode::PushTransform {
                transform: Transform {
                    translate_x: -layer_rect.x,
                    translate_y: -layer_rect.y,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotate: 0.0,
                    origin_x: 0.5,
                    origin_y: 0.5,
                },
            });
            Some(id)
        } else {
            None
        };
        // Handle modifier-based scroll containers.
        // This runs before the match on kind, so modifier scroll takes priority.
        if let Some(scroll) = &modifier.scroll {
            match scroll {
                ScrollBinding::Vertical(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_main {
                        s(vp.h.max(0.0));
                    }
                    let mut ch = 0.0f32;
                    for &c in &children {
                        let l = self.layout_for_node(c);
                        ch = ch.max(l.location.y + l.size.height);
                    }
                    if let Some(s) = &b.set_content_main {
                        s(ch);
                    }
                    let off = b.get_offset_main.as_ref().map(|f| f()).unwrap_or(0.0);

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });

                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0, child_offset_px.1 - off);

                    for &child_id in &children {
                        let l = self.layout_for_node(child_id);
                        let child_rect = repose_core::Rect {
                            x: scrolled_offset.0 + l.location.x,
                            y: scrolled_offset.1 + l.location.y,
                            w: l.size.width,
                            h: l.size.height,
                        };
                        if intersect_rect(child_rect, vp).is_none() {
                            self.stats.paint_culled += 1;
                            continue;
                        }
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }

                    clip_hits_to_viewport(hits, hits_start, vp);

                    if b.show_scrollbar {
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            ch,
                            off,
                            modifier.z_index,
                            ScrollbarAxis::V,
                            b.set_offset_main.clone(),
                        );
                    }

                    scene.nodes.push(SceneNode::PopClip);
                }
                ScrollBinding::Horizontal(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_main {
                        s(vp.w.max(0.0));
                    }
                    let mut cw = 0.0f32;
                    for &c in &children {
                        let l = self.layout_for_node(c);
                        cw = cw.max(l.location.x + l.size.width);
                    }
                    if let Some(s) = &b.set_content_main {
                        s(cw);
                    }
                    let off = b.get_offset_main.as_ref().map(|f| f()).unwrap_or(0.0);

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });

                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0 - off, child_offset_px.1);

                    for &child_id in &children {
                        let l = self.layout_for_node(child_id);
                        let child_rect = repose_core::Rect {
                            x: scrolled_offset.0 + l.location.x,
                            y: scrolled_offset.1 + l.location.y,
                            w: l.size.width,
                            h: l.size.height,
                        };
                        if intersect_rect(child_rect, vp).is_none() {
                            self.stats.paint_culled += 1;
                            continue;
                        }
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }

                    clip_hits_to_viewport(hits, hits_start, vp);

                    if b.show_scrollbar {
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            cw,
                            off,
                            modifier.z_index,
                            ScrollbarAxis::H,
                            b.set_offset_main.clone(),
                        );
                    }

                    scene.nodes.push(SceneNode::PopClip);
                }
                ScrollBinding::Both(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_width {
                        s(vp.w.max(0.0));
                    }
                    if let Some(s) = &b.set_viewport_height {
                        s(vp.h.max(0.0));
                    }
                    let mut cw = 0.0f32;
                    let mut ch = 0.0f32;
                    for &c in &children {
                        let mut stack: Vec<(NodeId, f32, f32)> = vec![(c, 0.0f32, 0.0f32)];
                        while let Some((cid, ox, oy)) = stack.pop() {
                            let l = self.layout_for_node(cid);
                            let ax = ox + l.location.x;
                            let ay = oy + l.location.y;
                            cw = cw.max(ax + l.size.width);
                            ch = ch.max(ay + l.size.height);
                            if let Some(node) = self.tree.get(cid) {
                                for &k in &node.children {
                                    stack.push((k, ax, ay));
                                }
                            }
                        }
                    }
                    if let Some(s) = &b.set_content_width {
                        s(cw);
                    }
                    if let Some(s) = &b.set_content_height {
                        s(ch);
                    }
                    let (ox, oy) = b.get_offset_xy.as_ref().map(|f| f()).unwrap_or((0.0, 0.0));

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });
                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0 - ox, child_offset_px.1 - oy);
                    for &child_id in &children {
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }
                    let mut i = hits_start;
                    while i < hits.len() {
                        if let Some(r) = intersect_rect(hits[i].rect, vp) {
                            hits[i].rect = r;
                            i += 1;
                        } else {
                            hits.remove(i);
                        }
                    }
                    if b.show_scrollbar {
                        let set_y = b
                            .set_offset_xy
                            .clone()
                            .map(|s| Rc::new(move |y| s(ox, y)) as Rc<dyn Fn(f32)>);
                        let set_x = b
                            .set_offset_xy
                            .clone()
                            .map(|s| Rc::new(move |x| s(x, oy)) as Rc<dyn Fn(f32)>);
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            ch,
                            oy,
                            modifier.z_index,
                            ScrollbarAxis::V,
                            set_y,
                        );
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            cw,
                            ox,
                            modifier.z_index,
                            ScrollbarAxis::H,
                            set_x,
                        );
                    }
                    scene.nodes.push(SceneNode::PopClip);
                }
            }
            // Pop focus group scope
            if modifier.focus_group {
                self.focus_group_stack.pop();
            }
            // Pop layer if present
            if let Some(id) = layer_id {
                scene.nodes.push(SceneNode::PopTransform);
                scene.nodes.push(SceneNode::EndLayer { layer_id: id });
            }
            // Pop clips and transforms pushed before the scroll branch
            if push_bounds_clip {
                scene.nodes.push(SceneNode::PopClip);
            }
            if modifier.clip_rect.is_some() && overflow_clip {
                scene.nodes.push(SceneNode::PopClip);
            }
            if push_round_clip && overflow_clip {
                scene.nodes.push(SceneNode::PopClip);
            }
            if modifier.transform.is_some() {
                scene.nodes.push(SceneNode::PopTransform);
            }
            // Wire up FocusRequester if present on the modifier
            set_focus_requester(&modifier, view_id);
            if let Some(cb) = &modifier.on_focus_changed {
                self.focus_callbacks.insert(view_id, cb.clone());
            }
            return;
        }
        // Non-scroll children
        // For non-scroll containers, the ViewKind determines the children walk pattern.
        match &kind {
            ViewKind::OverlayHost => {
                for &child_id in &children {
                    self.walk_paint(
                        child_id,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
            }
            ViewKind::Expander { expanded, .. } => {
                if let Some(&first) = children.first() {
                    self.walk_paint(
                        first,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
                if *expanded {
                    for &child_id in children.iter().skip(1) {
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            child_offset_px,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }
                }
            }
            _ => {
                for &child_id in &children {
                    self.walk_paint(
                        child_id,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
            }
        }

        // Pop focus group scope
        if modifier.focus_group {
            self.focus_group_stack.pop();
        }

        if let Some(id) = layer_id {
            scene.nodes.push(SceneNode::PopTransform);
            scene.nodes.push(SceneNode::EndLayer { layer_id: id });
            if let Some(shadow) = &modifier.shadow {
                scene.nodes.push(SceneNode::CompositeShadow {
                    layer_id: id,
                    blur_px: dp_to_px(shadow.blur_radius),
                    offset_px: (0.0, dp_to_px(shadow.offset_y)),
                    color: shadow.color,
                });
            }
        }

        if push_bounds_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if modifier.clip_rect.is_some() && overflow_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if push_round_clip && overflow_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if modifier.transform.is_some() {
            scene.nodes.push(SceneNode::PopTransform);
        }

        // Wire up FocusRequester if present on the modifier
        set_focus_requester(&modifier, view_id);

        if let Some(cb) = &modifier.on_focus_changed {
            self.focus_callbacks.insert(view_id, cb.clone());
        }
    }
}
