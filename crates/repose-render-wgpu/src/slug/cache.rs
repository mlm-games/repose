use std::collections::HashMap;
use std::collections::hash_map::Entry;

use cosmic_text::{CacheKey, Command};
use lyon_path::math::Point;
use lyon_tessellation::{
    FillOptions, FillTessellator, StrokeOptions, StrokeTessellator, VertexBuffers,
    geometry_builder::simple_builder,
};

use crate::slug::outline::commands_to_path;

const EVICT_FRAMES: u64 = 120;

/// Tessellated geometry for a single glyph, in em-space (font Y-up).
pub struct CachedTessGlyph {
    /// Expanded fill vertices (each triangle has 3 separate entries) in em-space.
    /// Computed lazily on first fill request.
    pub fill_vertices: Option<Vec<[f32; 2]>>,
    /// Expanded stroke vertices (each triangle has 3 separate entries) in em-space.
    /// Computed lazily on first stroke request.
    pub stroke_vertices: Option<Vec<[f32; 2]>>,
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
                    stroke_vertices: None,
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
    pub fn get_or_insert_stroke(
        &mut self,
        key: CacheKey,
        font_size: f32,
        commands: &[Command],
    ) -> Option<&CachedTessGlyph> {
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let glyph = e.get_mut();
                glyph.last_used = self.frame;
                if glyph.stroke_vertices.is_none() {
                    let path = commands_to_path(commands, font_size)?;
                    let mut tess = StrokeTessellator::new();
                    let tolerance = (0.5 / font_size).max(0.001);
                    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                    tess.tessellate_path(
                        &path,
                        &StrokeOptions::default()
                            .with_tolerance(tolerance)
                            .with_line_width(0.05),
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
                    glyph.stroke_vertices = Some(vertices);
                }
                Some(e.into_mut())
            }
            Entry::Vacant(e) => {
                let path = commands_to_path(commands, font_size)?;
                let mut tess = StrokeTessellator::new();
                let tolerance = (0.5 / font_size).max(0.001);
                let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
                tess.tessellate_path(
                    &path,
                    &StrokeOptions::default()
                        .with_tolerance(tolerance)
                        .with_line_width(0.05),
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
                    fill_vertices: None,
                    stroke_vertices: Some(vertices),
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
