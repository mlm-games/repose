use std::collections::HashMap;
use std::collections::hash_map::Entry;

use lyon_path::FillRule;
use lyon_path::math::Point;
use lyon_tessellation::{
    FillOptions, FillTessellator, LineCap, LineJoin, StrokeOptions, StrokeTessellator,
    VertexBuffers, geometry_builder::simple_builder,
};
use repose_text::{CacheKey, Command};

use crate::slug::outline::commands_to_path;
use crate::slug::path_effect::apply_path_effect;

const EVICT_FRAMES: u64 = 120;
/// Glyph outlines are TrueType/CFF and are painted with the nonzero winding
/// rule. Lyon defaults to even-odd, which punches a hole wherever two
/// same-direction contours of one glyph overlap, so counters and joins tear
/// open (Fredoka's `A` and `1`, `b`, `d`, `g`, `p`, `q`, `x`, `y`, ...).
fn glyph_fill_options(tolerance: f32) -> FillOptions {
    FillOptions::default()
        .with_tolerance(tolerance)
        .with_fill_rule(FillRule::NonZero)
}
/// Stroke variants kept per glyph. A glyph that is on screen while its
/// width or dash phase animates mints a new variant per frame, and eviction
/// only ever drops whole glyphs, so without this cap a single visible glyph
/// grows without bound.
const MAX_STROKE_VARIANTS_PER_GLYPH: usize = 8;
/// Total CPU bytes across all tessellated stroke variants.
const MAX_STROKE_VARIANT_BYTES: u64 = 32 * 1024 * 1024;
/// Bytes per `[f32; 2]` vertex.
const VERTEX_BYTES: u64 = 8;

/// Exact, hashable form of a path effect. Storing the float bits rather than
/// a digest keeps `Eq` faithful: a hash collision would otherwise serve one
/// effect's tessellation for another.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum StrokePathEffect {
    Corner {
        radius_bits: u32,
    },
    Dash {
        interval_bits: Vec<u32>,
        phase_bits: u32,
    },
}

fn path_effect_key(effect: &Option<repose_core::PathEffect>) -> Option<StrokePathEffect> {
    use repose_core::PathEffect;
    match effect {
        None => None,
        Some(PathEffect::Corner { radius }) => Some(StrokePathEffect::Corner {
            radius_bits: radius.to_bits(),
        }),
        Some(PathEffect::Dash { intervals, phase }) => Some(StrokePathEffect::Dash {
            interval_bits: intervals.iter().map(|v| v.to_bits()).collect(),
            phase_bits: phase.to_bits(),
        }),
    }
}

/// Distinguishes stroke tessellation variants for the same glyph.
/// Includes all stroke parameters that affect the tessellated output.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StrokeTessKey {
    width_bits: u32,
    cap: u8,
    join: u8,
    miter_bits: u32,
    path_effect: Option<StrokePathEffect>,
}

impl StrokeTessKey {
    pub fn new(
        width: f32,
        cap: repose_core::StrokeCap,
        join: repose_core::StrokeJoin,
        miter: f32,
        path_effect: &Option<repose_core::PathEffect>,
    ) -> Self {
        Self {
            width_bits: width.to_bits(),
            cap: cap as u8,
            join: join as u8,
            miter_bits: miter.to_bits(),
            path_effect: path_effect_key(path_effect),
        }
    }
}

/// Tessellated geometry for a single glyph, in em-space (font Y-up).
pub struct CachedTessGlyph {
    /// Expanded fill vertices (each triangle has 3 separate entries) in em-space.
    /// Computed lazily on first fill request.
    pub fill_vertices: Option<Vec<[f32; 2]>>,
    /// Expanded stroke vertices keyed by stroke parameters, with the frame
    /// each variant was last requested (LRU order under cap pressure).
    pub stroke_variants: HashMap<StrokeTessKey, StrokeVariant>,
    pub last_used: u64,
}

#[derive(Clone)]
pub struct StrokeVariant {
    pub vertices: Vec<[f32; 2]>,
    last_used: u64,
}

impl StrokeVariant {
    fn bytes(&self) -> u64 {
        self.vertices.len() as u64 * VERTEX_BYTES
    }
}

impl CachedTessGlyph {
    fn stroke_bytes(&self) -> u64 {
        self.stroke_variants
            .values()
            .map(StrokeVariant::bytes)
            .sum()
    }

    /// Drop the least-recently-used variants until at most `max` remain.
    fn trim_stroke_variants(&mut self, max: usize) -> u64 {
        let mut freed: u64 = 0;
        while self.stroke_variants.len() > max {
            let Some((key, bytes)) = self.least_recently_used_variant() else {
                break;
            };
            self.stroke_variants.remove(&key);
            freed = freed.saturating_add(bytes);
        }
        freed
    }

    fn least_recently_used_variant(&self) -> Option<(StrokeTessKey, u64)> {
        self.stroke_variants
            .iter()
            .min_by_key(|(_, variant)| variant.last_used)
            .map(|(key, variant)| (key.clone(), variant.bytes()))
    }
}

pub struct GlyphSlugCache {
    map: HashMap<CacheKey, CachedTessGlyph>,
    frame: u64,
    stroke_bytes: u64,
}

impl GlyphSlugCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            frame: 0,
            stroke_bytes: 0,
        }
    }

    pub fn next_frame(&mut self) {
        self.evict_stale();
        self.frame += 1;
    }

    /// Get or create tessellated geometry for a glyph.
    /// `commands` are raw swash outline commands at the given `font_size`.
    fn tessellate_fill<'a>(
        font_size: f32,
        commands: &[Command],
        entry: Entry<'a, CacheKey, CachedTessGlyph>,
        frame: u64,
    ) -> Option<&'a CachedTessGlyph> {
        match entry {
            Entry::Occupied(mut e) => {
                let glyph = e.get_mut();
                glyph.last_used = frame;
                if glyph.fill_vertices.is_none() {
                    let path = commands_to_path(commands, font_size)?;
                    let mut tess = FillTessellator::new();
                    let tolerance = (0.5 / font_size).max(0.001);
                    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                    tess.tessellate_path(
                        &path,
                        &glyph_fill_options(tolerance),
                        &mut simple_builder(&mut buffers),
                    )
                    .ok()?;
                    if buffers.indices.is_empty() {
                        return None;
                    }
                    let num_verts = buffers.indices.len();
                    let mut vertices = Vec::with_capacity(num_verts);
                    for &i in &buffers.indices {
                        let v = &buffers.vertices[i as usize];
                        vertices.push([v.x, v.y]);
                    }
                    glyph.fill_vertices = Some(vertices);
                }
                Some(e.into_mut())
            }
            Entry::Vacant(e) => {
                let path = commands_to_path(commands, font_size)?;
                let mut tess = FillTessellator::new();
                let tolerance = (0.5 / font_size).max(0.001);
                let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                tess.tessellate_path(
                    &path,
                    &glyph_fill_options(tolerance),
                    &mut simple_builder(&mut buffers),
                )
                .ok()?;
                if buffers.indices.is_empty() {
                    return None;
                }
                let num_verts = buffers.indices.len();
                let mut vertices = Vec::with_capacity(num_verts);
                for &i in &buffers.indices {
                    let v = &buffers.vertices[i as usize];
                    vertices.push([v.x, v.y]);
                }
                Some(e.insert(CachedTessGlyph {
                    fill_vertices: Some(vertices),
                    stroke_variants: HashMap::new(),
                    last_used: frame,
                }))
            }
        }
    }

    /// Get or create fill-tessellated geometry for a glyph.
    pub fn get_or_insert(
        &mut self,
        key: CacheKey,
        font_size: f32,
        commands: &[Command],
    ) -> Option<&CachedTessGlyph> {
        Self::tessellate_fill(font_size, commands, self.map.entry(key), self.frame)
    }

    /// Get or create stroke-tessellated geometry for a glyph.
    /// `commands` are raw swash outline commands at the given `font_size`.
    ///
    /// Fill geometry stays lazy: a stroke-only glyph never pays for it.
    pub fn get_or_insert_stroke(
        &mut self,
        key: CacheKey,
        font_size: f32,
        commands: &[Command],
        width: f32,
        cap: repose_core::StrokeCap,
        join: repose_core::StrokeJoin,
        miter: f32,
        path_effect: &Option<repose_core::PathEffect>,
    ) {
        let tess_key = StrokeTessKey::new(width, cap, join, miter, path_effect);
        let frame = self.frame;
        // A glyph with no vector outline (space, NBSP) must not leave an empty entry
        // behind: the renderer keys off its presence and would skip the atlas
        // fallback it needs.
        if commands.is_empty() {
            return;
        }
        if let Some(glyph) = self.map.get_mut(&key) {
            glyph.last_used = frame;
            if let Some(variant) = glyph.stroke_variants.get_mut(&tess_key) {
                variant.last_used = frame;
                return;
            }
        }
        let Some(path) = commands_to_path(commands, font_size) else {
            return;
        };
        let tolerance = (0.5 / font_size).max(0.001);
        let path = match path_effect {
            Some(effect) => apply_path_effect(&path, effect, tolerance),
            None => path,
        };
        let options = StrokeOptions::DEFAULT
            .with_line_width(width)
            .with_line_cap(match cap {
                repose_core::StrokeCap::Round => LineCap::Round,
                repose_core::StrokeCap::Square => LineCap::Square,
                repose_core::StrokeCap::Butt => LineCap::Butt,
            })
            .with_line_join(match join {
                repose_core::StrokeJoin::Miter => LineJoin::Miter,
                repose_core::StrokeJoin::Round => LineJoin::Round,
                repose_core::StrokeJoin::Bevel => LineJoin::Bevel,
            })
            .with_miter_limit(miter)
            .with_tolerance(tolerance);
        let mut tess = StrokeTessellator::new();
        let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
        if tess
            .tessellate_path(&path, &options, &mut simple_builder(&mut buffers))
            .is_err()
            || buffers.indices.is_empty()
        {
            return;
        }
        let mut vertices = Vec::with_capacity(buffers.indices.len());
        for &i in &buffers.indices {
            let v = &buffers.vertices[i as usize];
            vertices.push([v.x, v.y]);
        }
        let bytes = vertices.len() as u64 * VERTEX_BYTES;
        self.evict_stroke_variants(bytes, &key);
        let glyph = self.map.entry(key).or_insert_with(|| CachedTessGlyph {
            fill_vertices: None,
            stroke_variants: HashMap::new(),
            last_used: frame,
        });
        let freed = glyph.trim_stroke_variants(MAX_STROKE_VARIANTS_PER_GLYPH - 1);
        self.stroke_bytes = self.stroke_bytes.saturating_sub(freed);
        self.stroke_bytes = self.stroke_bytes.saturating_add(bytes);
        glyph.last_used = frame;
        glyph.stroke_variants.insert(
            tess_key,
            StrokeVariant {
                vertices,
                last_used: frame,
            },
        );
    }

    pub fn get(&self, key: &CacheKey) -> Option<&CachedTessGlyph> {
        self.map.get(key)
    }

    /// Reserve room for `incoming` bytes of stroke geometry by dropping the
    /// least-recently-used variants across every glyph except `skip`. A
    /// variant is always insertable: if the single glyph being filled is
    /// itself over budget, it is trimmed by the per-glyph cap instead.
    fn evict_stroke_variants(&mut self, incoming: u64, skip: &CacheKey) {
        while self.stroke_bytes.saturating_add(incoming) > MAX_STROKE_VARIANT_BYTES {
            // Borrow the victim rather than cloning a key (and a dash
            // interval vector) for every variant of every glyph, once per
            // eviction.
            let mut oldest: Option<(u64, &CacheKey, &StrokeTessKey, u64)> = None;
            for (key, glyph) in &self.map {
                if key == skip {
                    continue;
                }
                for (variant_key, variant) in &glyph.stroke_variants {
                    if oldest.is_none_or(|(seen, ..)| variant.last_used < seen) {
                        oldest = Some((variant.last_used, key, variant_key, variant.bytes()));
                    }
                }
            }
            let Some((_, key, variant_key, bytes)) = oldest else {
                break;
            };
            let (key, variant_key) = (key.clone(), variant_key.clone());
            if let Some(glyph) = self.map.get_mut(&key) {
                glyph.stroke_variants.remove(&variant_key);
            }
            self.stroke_bytes = self.stroke_bytes.saturating_sub(bytes);
        }
    }

    fn evict_stale(&mut self) {
        let frame = self.frame;
        let mut freed: u64 = 0;
        self.map.retain(|_, glyph| {
            let keep = frame.wrapping_sub(glyph.last_used) < EVICT_FRAMES;
            if !keep {
                freed = freed.saturating_add(glyph.stroke_bytes());
            }
            keep
        });
        self.stroke_bytes = self.stroke_bytes.saturating_sub(freed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_commands() -> Vec<Command> {
        vec![
            Command::MoveTo(0.0, 0.0),
            Command::LineTo(0.5, 0.0),
            Command::LineTo(0.5, 0.5),
            Command::LineTo(0.0, 0.5),
            Command::Close,
        ]
    }

    fn key(glyph_id: u32) -> CacheKey {
        CacheKey {
            font_id: 1,
            glyph_id,
            font_size_bits: 16.0f32.to_bits(),
            variation: None,
        }
    }

    fn stroke(
        cache: &mut GlyphSlugCache,
        key: CacheKey,
        width: f32,
        effect: &Option<repose_core::PathEffect>,
    ) {
        cache.get_or_insert_stroke(
            key.clone(),
            16.0,
            &square_commands(),
            width,
            repose_core::StrokeCap::Butt,
            repose_core::StrokeJoin::Miter,
            4.0,
            effect,
        );
        cache.next_frame();
    }

    /// An animated stroke width must not grow the cache without bound.
    #[test]
    fn animated_stroke_width_keeps_variants_bounded() {
        let mut cache = GlyphSlugCache::new();
        let dash = Some(repose_core::PathEffect::Dash {
            intervals: vec![0.1, 0.1],
            phase: 0.0,
        });
        for step in 0..2000 {
            let width = 0.01 + (step % 977) as f32 * 0.000_37;
            stroke(&mut cache, key(1), width, &dash);
            let variants = cache
                .get(&key(1))
                .expect("glyph stays cached")
                .stroke_variants
                .len();
            assert!(
                variants <= MAX_STROKE_VARIANTS_PER_GLYPH,
                "{variants} variants after {step} distinct widths"
            );
        }
        assert!(cache.stroke_bytes <= MAX_STROKE_VARIANT_BYTES);
    }

    /// Every distinct parameter set must get its own variant (no Eq
    /// collisions between path effects).
    #[test]
    fn distinct_path_effects_do_not_share_a_variant() {
        let mut cache = GlyphSlugCache::new();
        let effects = [
            None,
            Some(repose_core::PathEffect::Corner { radius: 0.02 }),
            Some(repose_core::PathEffect::Dash {
                intervals: vec![0.1, 0.1],
                phase: 0.0,
            }),
            Some(repose_core::PathEffect::Dash {
                intervals: vec![0.1, 0.2],
                phase: 0.0,
            }),
            Some(repose_core::PathEffect::Dash {
                intervals: vec![0.1, 0.1],
                phase: 0.05,
            }),
        ];
        for (index, effect) in effects.iter().enumerate() {
            stroke(&mut cache, key(index as u32 + 1), 0.02, effect);
        }
        for (index, effect) in effects.iter().enumerate() {
            let cached = cache.get(&key(index as u32 + 1)).expect("glyph cached");
            assert_eq!(
                cached.stroke_variants.len(),
                1,
                "effect {index} should own exactly one variant"
            );
            assert_eq!(
                cached.stroke_variants.keys().next(),
                Some(&StrokeTessKey::new(
                    0.02,
                    repose_core::StrokeCap::Butt,
                    repose_core::StrokeJoin::Miter,
                    4.0,
                    effect
                ))
            );
        }
    }

    #[test]
    fn identical_parameters_reuse_one_variant() {
        let mut cache = GlyphSlugCache::new();
        let dash = Some(repose_core::PathEffect::Dash {
            intervals: vec![0.1, 0.1],
            phase: 0.0,
        });
        for _ in 0..10 {
            stroke(&mut cache, key(7), 0.02, &dash);
        }
        assert_eq!(cache.get(&key(7)).unwrap().stroke_variants.len(), 1);
    }
}
