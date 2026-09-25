use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use repose_core::{
    ClipOp, Frame, HitRegion, ImeAction, KeyboardActions, Rect, Sp, TextInputConfig, Transform,
    Vec2,
};

use crate::textfield::TextFieldMetrics;

#[derive(Clone)]
pub(crate) struct HitClip {
    world_to_local: [f64; 9],
    rect: Rect,
    op: ClipOp,
}

#[derive(Clone)]
struct HitClipLink {
    clip: HitClip,
    parent: Option<Arc<HitClipLink>>,
}

#[derive(Clone, Default)]
struct HitClipChain {
    head: Option<Arc<HitClipLink>>,
}

impl HitClipChain {
    fn iter(&self) -> HitClipIter<'_> {
        HitClipIter {
            next: self.head.as_deref(),
        }
    }
}

struct HitClipIter<'a> {
    next: Option<&'a HitClipLink>,
}

impl<'a> Iterator for HitClipIter<'a> {
    type Item = &'a HitClip;

    fn next(&mut self) -> Option<Self::Item> {
        let link = self.next?;
        self.next = link.parent.as_deref();
        Some(&link.clip)
    }
}

#[derive(Clone)]
pub(crate) struct HitContext {
    local_to_world: [f64; 9],
    world_to_local: [f64; 9],
    clips: HitClipChain,
    layer_depth: u32,
    valid: bool,
    cull_safe: bool,
    defer_safe: bool,
}

impl Default for HitContext {
    fn default() -> Self {
        Self::root()
    }
}

impl HitContext {
    pub(crate) fn root() -> Self {
        Self {
            local_to_world: identity_matrix(),
            world_to_local: identity_matrix(),
            clips: HitClipChain::default(),
            layer_depth: 0,
            valid: true,
            cull_safe: true,
            defer_safe: true,
        }
    }

    pub(crate) fn with_transform(&self, transform: Transform) -> Self {
        let local = transform.projective_matrix();
        let local_to_world =
            Transform::compose_projective(&self.local_to_world.map(|v| v as f32), &local)
                .map(|v| v as f64);
        let Some(world_to_local) = invert_matrix(local_to_world) else {
            return Self {
                local_to_world,
                world_to_local: self.world_to_local,
                clips: self.clips.clone(),
                layer_depth: self.layer_depth,
                valid: false,
                cull_safe: false,
                defer_safe: false,
            };
        };
        Self {
            local_to_world,
            world_to_local,
            clips: self.clips.clone(),
            layer_depth: self.layer_depth,
            valid: self.valid,
            cull_safe: self.cull_safe && !transform.has_perspective(),
            defer_safe: self.defer_safe && matrix_is_identity(local),
        }
    }

    pub(crate) fn can_defer(&self) -> bool {
        self.valid && self.defer_safe
    }

    pub(crate) fn with_layer(&self) -> Self {
        Self {
            local_to_world: self.local_to_world,
            world_to_local: self.world_to_local,
            clips: self.clips.clone(),
            layer_depth: self.layer_depth.saturating_add(1),
            valid: self.valid,
            cull_safe: self.cull_safe,
            defer_safe: false,
        }
    }

    pub(crate) fn with_clip(&self, rect: Rect, op: ClipOp) -> Self {
        let link = Arc::new(HitClipLink {
            clip: HitClip {
                world_to_local: self.world_to_local,
                rect,
                op,
            },
            parent: self.clips.head.clone(),
        });
        Self {
            local_to_world: self.local_to_world,
            world_to_local: self.world_to_local,
            clips: HitClipChain { head: Some(link) },
            layer_depth: self.layer_depth,
            valid: self.valid,
            cull_safe: self.cull_safe,
            defer_safe: false,
        }
    }

    pub(crate) fn project_rect(&self, rect: Rect) -> Rect {
        project_rect(self.local_to_world, rect)
    }

    pub(crate) fn to_local(&self, point: Vec2) -> Vec2 {
        apply_matrix(self.world_to_local, point)
    }

    pub(crate) fn intersects(&self, rect: Rect, world: Rect) -> bool {
        if !self.valid || !self.cull_safe {
            return true;
        }
        intersects_rect(self.project_rect(rect), world)
            && self
                .clips
                .iter()
                .filter(|clip| clip.op == ClipOp::Intersect)
                .all(|clip| {
                    invert_matrix(clip.world_to_local)
                        .map(|clip_to_world| {
                            intersects_rect(project_rect(clip_to_world, clip.rect), world)
                        })
                        .unwrap_or(false)
                })
    }

    pub(crate) fn cache_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for value in self.local_to_world {
            value.to_bits().hash(&mut hasher);
        }
        for value in self.world_to_local {
            value.to_bits().hash(&mut hasher);
        }
        let mut clip_count = 0usize;
        for clip in self.clips.iter() {
            clip_count += 1;
            for value in clip.world_to_local {
                value.to_bits().hash(&mut hasher);
            }
            clip.rect.x.to_bits().hash(&mut hasher);
            clip.rect.y.to_bits().hash(&mut hasher);
            clip.rect.w.to_bits().hash(&mut hasher);
            clip.rect.h.to_bits().hash(&mut hasher);
            match clip.op {
                ClipOp::Intersect => 0u8.hash(&mut hasher),
                ClipOp::Difference => 1u8.hash(&mut hasher),
            }
        }
        clip_count.hash(&mut hasher);
        self.layer_depth.hash(&mut hasher);
        self.valid.hash(&mut hasher);
        self.cull_safe.hash(&mut hasher);
        self.defer_safe.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Clone)]
pub(crate) struct HitRegionMetadata {
    local_rect: Rect,
    world_rect: Rect,
    local_to_world: [f64; 9],
    world_to_local: [f64; 9],
    clips: HitClipChain,
    textfield_metrics: Option<TextFieldMetrics>,
    keyboard_actions: Option<KeyboardActions>,
    textfield_transform_id: Option<u64>,
}

#[derive(Clone)]
pub struct HitRegionSnapshot {
    hit: HitRegion,
    metadata: Option<Arc<HitRegionMetadata>>,
}

impl HitRegionSnapshot {
    pub fn new(hit: &HitRegion) -> Self {
        Self {
            hit: hit.clone(),
            metadata: metadata_for(hit),
        }
    }

    pub fn id(&self) -> u64 {
        self.hit.id
    }

    pub fn hit(&self) -> &HitRegion {
        &self.hit
    }

    pub fn contains(&self, position: Vec2) -> bool {
        contains(&self.hit, self.metadata.as_deref(), position)
    }

    pub fn to_local(&self, position: Vec2) -> Vec2 {
        to_local(&self.hit, self.metadata.as_deref(), position)
    }

    pub fn pointer_coordinates(&self, position: Vec2) -> (Vec2, Vec2) {
        pointer_coordinates(&self.hit, self.metadata.as_deref(), position)
    }
}

thread_local! {
    static HIT_METADATA: RefCell<HashMap<u64, (Arc<HitRegionMetadata>, u64)>> =
        RefCell::new(HashMap::new());
    static HIT_METADATA_GENERATION: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn begin_hit_metadata_frame() {
    HIT_METADATA_GENERATION.with(|generation| {
        generation.set(generation.get().wrapping_add(1));
    });
}

pub(crate) fn prune_hit_metadata() {
    let generation = HIT_METADATA_GENERATION.with(Cell::get);
    HIT_METADATA.with(|metadata| {
        metadata
            .borrow_mut()
            .retain(|_, (_, stamp)| *stamp == generation);
    });
}

pub(crate) fn register_hit(
    hit: &mut HitRegion,
    context: &HitContext,
    text_input: Option<&TextInputConfig>,
) {
    let local_rect = hit.rect;
    let mut metadata = HitRegionMetadata {
        local_rect,
        world_rect: Rect::default(),
        local_to_world: context.local_to_world,
        world_to_local: context.world_to_local,
        clips: context.clips.clone(),
        textfield_metrics: text_input.map(TextFieldMetrics::from_config),
        keyboard_actions: text_input.and_then(|input| input.keyboard_actions.clone()),
        textfield_transform_id: text_input
            .and_then(|input| input.visual_transformation.as_ref())
            .and_then(crate::textfield::textfield_transform_id),
    };
    metadata.world_rect = metadata.compute_world_rect();
    hit.rect = metadata.world_rect;
    let generation = HIT_METADATA_GENERATION.with(Cell::get);
    HIT_METADATA.with(|all| {
        all.borrow_mut()
            .insert(hit.id, (Arc::new(metadata), generation));
    });
}

fn with_metadata<R>(hit: &HitRegion, f: impl FnOnce(&HitRegionMetadata) -> R) -> Option<R> {
    HIT_METADATA.with(|all| {
        let all = all.borrow();
        let (metadata, _) = all.get(&hit.id)?;
        (metadata.world_rect == hit.rect).then(|| f(metadata))
    })
}

pub(crate) fn metadata_for(hit: &HitRegion) -> Option<Arc<HitRegionMetadata>> {
    HIT_METADATA.with(|all| {
        let all = all.borrow();
        let (metadata, _) = all.get(&hit.id)?;
        (metadata.world_rect == hit.rect).then(|| Arc::clone(metadata))
    })
}

pub(crate) fn restore_metadata(id: u64, metadata: Arc<HitRegionMetadata>) {
    let generation = HIT_METADATA_GENERATION.with(Cell::get);
    HIT_METADATA.with(|all| {
        all.borrow_mut().insert(id, (metadata, generation));
    });
}

pub fn hit_test_frame(frame: &Frame, position: Vec2) -> Option<&HitRegion> {
    frame_hit_order(frame, position, true)
        .first()
        .copied()
        .and_then(|index| frame.hit_regions.get(index))
}

pub fn hit_test_enabled_frame(frame: &Frame, position: Vec2) -> Option<&HitRegion> {
    frame_hit_order(frame, position, false)
        .first()
        .copied()
        .and_then(|index| frame.hit_regions.get(index))
}

pub fn hit_test_frame_path(frame: &Frame, position: Vec2) -> Vec<u64> {
    hit_test_path(frame, position, false)
}

pub fn hit_test_frame_regions(frame: &Frame, position: Vec2) -> Vec<u64> {
    frame_hit_order(frame, position, false)
        .into_iter()
        .map(|index| frame.hit_regions[index].id)
        .collect()
}

pub fn hit_region_contains(hit: &HitRegion, position: Vec2) -> bool {
    with_metadata(hit, |metadata| contains(hit, Some(metadata), position))
        .unwrap_or_else(|| hit.rect.contains(position))
}

pub fn hit_region_to_local(hit: &HitRegion, position: Vec2) -> Vec2 {
    with_metadata(hit, |metadata| to_local(hit, Some(metadata), position)).unwrap_or(position)
}

pub fn hit_region_pointer_coordinates(hit: &HitRegion, position: Vec2) -> (Vec2, Vec2) {
    with_metadata(hit, |metadata| {
        pointer_coordinates(hit, Some(metadata), position)
    })
    .unwrap_or_else(|| {
        let origin = Vec2 {
            x: hit.rect.x,
            y: hit.rect.y,
        };
        (origin, position - origin)
    })
}

pub fn hit_region_local_rect(hit: &HitRegion) -> Rect {
    with_metadata(hit, |metadata| metadata.local_rect).unwrap_or(hit.rect)
}

pub fn hit_region_transformed_origin(hit: &HitRegion) -> Vec2 {
    with_metadata(hit, |metadata| {
        apply_matrix(
            metadata.local_to_world,
            Vec2 {
                x: metadata.local_rect.x,
                y: metadata.local_rect.y,
            },
        )
    })
    .unwrap_or(Vec2 {
        x: hit.rect.x,
        y: hit.rect.y,
    })
}

pub fn hit_region_world_rect(hit: &HitRegion) -> Rect {
    with_metadata(hit, |metadata| metadata.world_rect).unwrap_or(hit.rect)
}

pub fn dispatch_keyboard_action(hit: &HitRegion, action: ImeAction, default: &dyn Fn()) -> bool {
    let callback = with_metadata(hit, |metadata| {
        metadata.keyboard_actions.as_ref().and_then(|actions| {
            let callback = match action {
                ImeAction::Done => &actions.on_done,
                ImeAction::Go => &actions.on_go,
                ImeAction::Next => &actions.on_next,
                ImeAction::Previous => &actions.on_previous,
                ImeAction::Search => &actions.on_search,
                ImeAction::Send => &actions.on_send,
                ImeAction::Unspecified | ImeAction::None | ImeAction::Default => return None,
            };
            callback.clone()
        })
    });
    let Some(callback) = callback else {
        default();
        return true;
    };
    if let Some(callback) = callback {
        let scope = DefaultKeyboardActionScope { action, default };
        callback(&scope);
    } else {
        default();
    }
    true
}

pub(crate) fn textfield_metrics(hit: &HitRegion) -> TextFieldMetrics {
    with_metadata(hit, |metadata| metadata.textfield_metrics.clone())
        .flatten()
        .unwrap_or_else(|| {
            let mut metrics = TextFieldMetrics::default();
            if hit.tf_font_size != Sp::ZERO {
                metrics.font_px = hit.tf_font_size.to_px().0;
                metrics.line_height_px = metrics.font_px;
            }
            metrics
        })
}

pub(crate) fn textfield_transform_id(hit: &HitRegion) -> Option<u64> {
    with_metadata(hit, |metadata| metadata.textfield_transform_id).flatten()
}

fn contains(hit: &HitRegion, metadata: Option<&HitRegionMetadata>, position: Vec2) -> bool {
    let Some(metadata) = metadata else {
        return hit.rect.contains(position);
    };
    let local = apply_matrix(metadata.world_to_local, position);
    metadata.local_rect.contains(local)
        && metadata
            .clips
            .iter()
            .all(|clip| clip_allows(clip, position))
}

fn to_local(_hit: &HitRegion, metadata: Option<&HitRegionMetadata>, position: Vec2) -> Vec2 {
    metadata.map_or(position, |metadata| {
        apply_matrix(metadata.world_to_local, position)
    })
}

fn pointer_coordinates(
    hit: &HitRegion,
    metadata: Option<&HitRegionMetadata>,
    position: Vec2,
) -> (Vec2, Vec2) {
    let Some(metadata) = metadata else {
        let origin = Vec2 {
            x: hit.rect.x,
            y: hit.rect.y,
        };
        return (origin, position - origin);
    };
    let local = apply_matrix(metadata.world_to_local, position);
    let local_origin = Vec2 {
        x: metadata.local_rect.x,
        y: metadata.local_rect.y,
    };
    let local_position = local - local_origin;
    let origin = apply_matrix(metadata.local_to_world, local_origin);
    (origin, local_position)
}

fn hit_test_path(frame: &Frame, position: Vec2, include_disabled: bool) -> Vec<u64> {
    let order = frame_hit_order(frame, position, include_disabled);
    let Some(&top) = order.first() else {
        return Vec::new();
    };
    let exact: HashSet<u64> = order
        .iter()
        .map(|index| frame.hit_regions[*index].id)
        .collect();
    let by_id: HashMap<u64, &HitRegion> =
        frame.hit_regions.iter().map(|hit| (hit.id, hit)).collect();
    let mut path = Vec::new();
    let mut current = Some(frame.hit_regions[top].id);
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            break;
        }
        let Some(hit) = by_id.get(&id) else {
            break;
        };
        if exact.contains(&id) {
            path.push(id);
        }
        current = hit.parent;
    }
    path
}

fn frame_hit_order(frame: &Frame, position: Vec2, include_disabled: bool) -> Vec<usize> {
    let mut order = HIT_METADATA.with(|all| {
        let all = all.borrow();
        frame
            .hit_regions
            .iter()
            .enumerate()
            .filter(|(_, hit)| include_disabled || !hit.disabled)
            .filter(|(_, hit)| {
                all.get(&hit.id)
                    .filter(|(metadata, _)| metadata.world_rect == hit.rect)
                    .map(|(metadata, _)| contains(hit, Some(metadata), position))
                    .unwrap_or_else(|| hit.rect.contains(position))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    });
    order.sort_unstable_by(|left, right| {
        let left_hit = &frame.hit_regions[*left];
        let right_hit = &frame.hit_regions[*right];
        right_hit
            .z_index
            .partial_cmp(&left_hit.z_index)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.cmp(left))
    });
    order
}

struct DefaultKeyboardActionScope<'a> {
    action: ImeAction,
    default: &'a dyn Fn(),
}

impl repose_core::KeyboardActionScope for DefaultKeyboardActionScope<'_> {
    fn default_keyboard_action(&self, action: ImeAction) {
        if action == self.action {
            (self.default)();
        }
    }
}

impl HitRegionMetadata {
    fn compute_world_rect(&self) -> Rect {
        let mut world = project_rect(self.local_to_world, self.local_rect);
        for clip in self
            .clips
            .iter()
            .filter(|clip| clip.op == ClipOp::Intersect)
        {
            let Some(clip_to_world) = invert_matrix(clip.world_to_local) else {
                continue;
            };
            let projected = project_rect(clip_to_world, clip.rect);
            world = intersect_rect(world, projected).unwrap_or(Rect {
                x: world.x,
                y: world.y,
                w: 0.0,
                h: 0.0,
            });
            if world.w <= 0.0 || world.h <= 0.0 {
                break;
            }
        }
        world
    }
}

fn intersects_rect(a: Rect, b: Rect) -> bool {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    right > x && bottom > y
}

fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (right > x && bottom > y).then_some(Rect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    })
}

fn clip_allows(clip: &HitClip, position: Vec2) -> bool {
    let local = apply_matrix(clip.world_to_local, position);
    if clip.op == ClipOp::Difference {
        !clip.rect.contains(local)
    } else {
        clip.rect.contains(local)
    }
}

fn project_rect(matrix: [f64; 9], rect: Rect) -> Rect {
    let corners = [
        Vec2 {
            x: rect.x,
            y: rect.y,
        },
        Vec2 {
            x: rect.x + rect.w,
            y: rect.y,
        },
        Vec2 {
            x: rect.x + rect.w,
            y: rect.y + rect.h,
        },
        Vec2 {
            x: rect.x,
            y: rect.y + rect.h,
        },
    ];
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for corner in corners {
        let point = apply_matrix(matrix, corner);
        min_x = min_x.min(point.x as f64);
        min_y = min_y.min(point.y as f64);
        max_x = max_x.max(point.x as f64);
        max_y = max_y.max(point.y as f64);
    }
    Rect {
        x: min_x as f32,
        y: min_y as f32,
        w: (max_x - min_x) as f32,
        h: (max_y - min_y) as f32,
    }
}

fn identity_matrix() -> [f64; 9] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
}

fn matrix_is_identity(matrix: [f32; 9]) -> bool {
    matrix
        .iter()
        .zip([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
        .all(|(left, right)| *left == right)
}

fn apply_matrix(matrix: [f64; 9], point: Vec2) -> Vec2 {
    let x = point.x as f64;
    let y = point.y as f64;
    let w = matrix[6] * x + matrix[7] * y + matrix[8];
    let w = if w.abs() < 1e-9 {
        if w < 0.0 { -1e-9 } else { 1e-9 }
    } else {
        w
    };
    Vec2 {
        x: ((matrix[0] * x + matrix[1] * y + matrix[2]) / w) as f32,
        y: ((matrix[3] * x + matrix[4] * y + matrix[5]) / w) as f32,
    }
}

fn invert_matrix(matrix: [f64; 9]) -> Option<[f64; 9]> {
    let a = matrix[0];
    let b = matrix[1];
    let c = matrix[2];
    let d = matrix[3];
    let e = matrix[4];
    let f = matrix[5];
    let g = matrix[6];
    let h = matrix[7];
    let i = matrix[8];
    let c00 = e * i - f * h;
    let c01 = -(d * i - f * g);
    let c02 = d * h - e * g;
    let det = a * c00 + b * c01 + c * c02;
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        c00 * inv,
        -(b * i - c * h) * inv,
        (b * f - c * e) * inv,
        c01 * inv,
        (a * i - c * g) * inv,
        -(a * f - c * d) * inv,
        c02 * inv,
        -(a * h - b * g) * inv,
        (a * e - b * d) * inv,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_coordinates_preserve_global_position_with_transform() {
        let mut hit = HitRegion {
            id: 9_876_543,
            rect: Rect {
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 50.0,
            },
            ..Default::default()
        };
        let context = HitContext::root().with_transform(Transform::translate(30.0, 40.0));
        register_hit(&mut hit, &context, None);

        let global = Vec2 { x: 50.0, y: 70.0 };
        let (origin, local) = hit_region_pointer_coordinates(&hit, global);
        assert_eq!(origin, Vec2 { x: 40.0, y: 60.0 });
        assert_eq!(origin + local, global);

        let mut event = crate::PointerEvent::new(
            crate::PointerId(1),
            crate::PointerKind::Mouse,
            crate::PointerEventKind::Move,
            global,
            1.0,
            crate::Modifiers::default(),
        );
        event.origin = origin;
        event.position = local;
        assert_eq!(event.position_in_window(), global);

        let mut scaled = HitRegion {
            id: 9_876_544,
            rect: Rect {
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 50.0,
            },
            ..Default::default()
        };
        let scaled_context = HitContext::root().with_transform(Transform {
            scale_x: 2.0,
            scale_y: 2.0,
            ..Transform::identity()
        });
        register_hit(&mut scaled, &scaled_context, None);
        let (origin, local) = hit_region_pointer_coordinates(&scaled, global);
        let mut event = crate::PointerEvent::new(
            crate::PointerId(2),
            crate::PointerKind::Mouse,
            crate::PointerEventKind::Move,
            global,
            1.0,
            crate::Modifiers::default(),
        );
        event.origin = origin;
        event.position = local;
        assert_eq!(event.position_in_window(), global);
    }
}
