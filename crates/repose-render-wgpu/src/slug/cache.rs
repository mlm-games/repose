use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};

use lyon_path::Path;
use lyon_path::math::Point;
use lyon_tessellation::{
    FillOptions, FillTessellator, LineCap, LineJoin, StrokeOptions, StrokeTessellator,
    VertexBuffers, geometry_builder::simple_builder,
};
use repose_text::{CacheKey, Command};

use crate::slug::outline::commands_to_path;
use crate::slug::path_effect::apply_path_effect;

const EVICT_FRAMES: u64 = 120;

/// Distinguishes stroke tessellation variants for the same glyph.
/// Includes all stroke parameters that affect the tessellated output.
#[derive(Clone, PartialEq, Eq)]
pub struct StrokeTessKey {
    width_bits: u32,
    cap: u8,
    join: u8,
    miter_bits: u32,
    /// Hash of the path effect (f32 values converted via to_bits).
    path_effect_hash: u64,
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
            path_effect_hash: hash_path_effect(path_effect),
        }
    }
}

impl Hash for StrokeTessKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width_bits.hash(state);
        self.cap.hash(state);
        self.join.hash(state);
        self.miter_bits.hash(state);
        self.path_effect_hash.hash(state);
    }
}

fn hash_path_effect(effect: &Option<repose_core::PathEffect>) -> u64 {
    use repose_core::PathEffect;
    match effect {
        None => 0,
        Some(PathEffect::Corner { radius }) => {
            1u64.wrapping_mul(31).wrapping_add(radius.to_bits() as u64)
        }
        Some(PathEffect::Dash { intervals, phase }) => {
            let mut h: u64 = 2;
            for &v in intervals {
                h = h.wrapping_mul(31).wrapping_add(v.to_bits() as u64);
            }
            h.wrapping_mul(31).wrapping_add(phase.to_bits() as u64)
        }
    }
}

/// Tessellated geometry for a single glyph, in em-space (font Y-up).
pub struct CachedTessGlyph {
    /// Expanded fill vertices (each triangle has 3 separate entries) in em-space.
    /// Computed lazily on first fill request.
    pub fill_vertices: Option<Vec<[f32; 2]>>,
    /// Expanded stroke vertices keyed by stroke parameters.
    pub stroke_variants: HashMap<StrokeTessKey, Vec<[f32; 2]>>,
    pub last_used: u64,
}

pub struct GlyphSlugCache {
    map: HashMap<CacheKey, CachedTessGlyph>,
    frame: u64,
}

impl GlyphSlugCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            frame: 0,
        }
    }

    pub fn next_frame(&mut self) {
        self.evict_stale();
        self.frame += 1;
    }

    pub fn contains(&self, key: &CacheKey) -> bool {
        self.map.contains_key(key)
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
                        &FillOptions::default().with_tolerance(tolerance),
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
                    &FillOptions::default().with_tolerance(tolerance),
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
    /// `width_em` is the stroke width in em-units (fraction of font size).
    pub fn get_or_insert_stroke(
        &mut self,
        key: CacheKey,
        font_size: f32,
        commands: &[Command],
        width_em: f32,
        cap: repose_core::StrokeCap,
        join: repose_core::StrokeJoin,
        miter: f32,
        path_effect: &Option<repose_core::PathEffect>,
    ) -> Option<&CachedTessGlyph> {
        let tess_key = StrokeTessKey::new(width_em, cap, join, miter, path_effect);
        let lyon_cap = match cap {
            repose_core::StrokeCap::Butt => LineCap::Butt,
            repose_core::StrokeCap::Round => LineCap::Round,
            repose_core::StrokeCap::Square => LineCap::Square,
        };
        let lyon_join = match join {
            repose_core::StrokeJoin::Miter => LineJoin::Miter,
            repose_core::StrokeJoin::Round => LineJoin::Round,
            repose_core::StrokeJoin::Bevel => LineJoin::Bevel,
        };
        let build_path = |font_size| -> Option<Path> {
            let path = commands_to_path(commands, font_size)?;
            if let Some(effect) = path_effect {
                let tolerance = (0.5 / font_size).max(0.001);
                Some(apply_path_effect(&path, effect, tolerance))
            } else {
                Some(path)
            }
        };
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let glyph = e.get_mut();
                glyph.last_used = self.frame;
                if !glyph.stroke_variants.contains_key(&tess_key) {
                    let path = build_path(font_size)?;
                    let mut tess = StrokeTessellator::new();
                    let tolerance = (0.5 / font_size).max(0.001);
                    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                    tess.tessellate_path(
                        &path,
                        &StrokeOptions::default()
                            .with_tolerance(tolerance)
                            .with_line_width(width_em)
                            .with_line_cap(lyon_cap)
                            .with_line_join(lyon_join)
                            .with_miter_limit(miter),
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
                    glyph.stroke_variants.insert(tess_key, vertices);
                }
                Some(e.into_mut())
            }
            Entry::Vacant(e) => {
                let path = build_path(font_size)?;
                let mut tess = StrokeTessellator::new();
                let tolerance = (0.5 / font_size).max(0.001);
                let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                tess.tessellate_path(
                    &path,
                    &StrokeOptions::default()
                        .with_tolerance(tolerance)
                        .with_line_width(width_em)
                        .with_line_cap(lyon_cap)
                        .with_line_join(lyon_join)
                        .with_miter_limit(miter),
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
                let mut variants = HashMap::new();
                variants.insert(tess_key, vertices);
                Some(e.insert(CachedTessGlyph {
                    fill_vertices: None,
                    stroke_variants: variants,
                    last_used: self.frame,
                }))
            }
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<&CachedTessGlyph> {
        self.map.get(key)
    }

    pub fn touch(&mut self, key: &CacheKey) {
        if let Some(entry) = self.map.get_mut(key) {
            entry.last_used = self.frame;
        }
    }

    fn evict_stale(&mut self) {
        let frame = self.frame;
        self.map
            .retain(|_, e| frame.wrapping_sub(e.last_used) < EVICT_FRAMES);
    }
}
