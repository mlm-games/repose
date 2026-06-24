use std::collections::HashMap;
use std::collections::hash_map::Entry;

use cosmic_text::{CacheKey, Command};

use crate::slug::band::{BandData, build_bands};
use crate::slug::outline::{QuadCurve, commands_to_curves};

/// Slug glyph header layout (u32 offsets from base):
///   0: h_count(8) | v_count(8) | num_curves(16)
///   1: band_scale.x   2: band_scale.y
///   3: band_offset.x   4: band_offset.y
///   5..8: bounds [xmin, ymin, xmax, ymax]
///   9: h_max(16) | v_max(16)
///  10: font_size_bits
///  11: curve_start (absolute u32 offset to first curve)
///  12+: band headers (count + ref_off packed), then refs, then curve data
const HEADER_WORDS: u32 = 12;
const EVICT_FRAMES: u64 = 120;
const SHRINK_FACTOR: u64 = 4;
const MIN_STORAGE: u32 = 65536;

/// Cached vector data for a glyph, with a fixed GPU storage offset.
pub struct CachedGlyphSlug {
    pub curves: Vec<QuadCurve>,
    pub band_data: BandData,
    /// Offset in u32 units into the GPU storage buffer.
    pub gpu_offset: u32,
    pub last_used: u64,
    /// Pixel font size, for coverage scaling in the shader.
    pub font_size_bits: u32,
}

/// Manages CPU-side outline cache + GPU storage buffer allocation.
pub struct GlyphSlugCache {
    map: HashMap<CacheKey, CachedGlyphSlug>,
    frame: u64,
    next_offset: u32,
    dirty: bool,
    sorted_keys: Vec<CacheKey>,
    /// Monotonically increasing counter bumped every time storage contents change.
    /// Lets the upload path skip `write_buffer` when nothing changed.
    pub generation: u64,
}

impl GlyphSlugCache {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            frame: 0,
            next_offset: 0,
            dirty: false,
            sorted_keys: Vec::new(),
            generation: 0,
        }
    }

    /// Call at the start of each frame, before scene traversal.
    pub fn next_frame(&mut self) {
        self.evict_stale();
        self.frame += 1;
    }

    fn is_alive(&self, entry: &CachedGlyphSlug) -> bool {
        entry.last_used == self.frame || self.frame.wrapping_sub(entry.last_used) < EVICT_FRAMES
    }

    fn compute_block_size(curves: &[QuadCurve], bd: &BandData) -> u32 {
        let num_bands = bd.h_count + bd.v_count;
        let total_refs: usize = bd
            .h_bands
            .iter()
            .chain(bd.v_bands.iter())
            .map(|b| b.indices.len())
            .sum();
        HEADER_WORDS + num_bands + total_refs as u32 + curves.len() as u32 * 6
    }

    pub fn contains(&self, key: &CacheKey) -> bool {
        self.map.contains_key(key)
    }

    /// Get or create cached outline data for a glyph.
    /// `commands` are the raw swash outline commands at the given `font_size`.
    /// Caller should only call this on cache miss; the commands argument is
    /// consumed only on the Vacant branch.
    pub fn get_or_insert(
        &mut self,
        key: CacheKey,
        font_size: f32,
        commands: &[Command],
    ) -> Option<&CachedGlyphSlug> {
        match self.map.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().last_used = self.frame;
                Some(entry.into_mut())
            }
            Entry::Vacant(entry) => {
                let curves = commands_to_curves(commands, font_size)?;
                let band_data = build_bands(&curves);
                let block_size = Self::compute_block_size(&curves, &band_data);
                let gpu_offset = self.next_offset;
                self.next_offset += block_size;
                self.dirty = true;
                self.generation = self.generation.wrapping_add(1);
                Some(entry.insert(CachedGlyphSlug {
                    curves,
                    band_data,
                    gpu_offset,
                    last_used: self.frame,
                    font_size_bits: font_size.to_bits(),
                }))
            }
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<&CachedGlyphSlug> {
        self.map.get(key)
    }

    /// Build the storage buffer contents for all in-cache glyphs.
    /// Sorts keys only when the cache was modified.
    /// The returned Vec is empty when no glyphs are alive.
    pub fn build_storage(&mut self) -> Vec<u32> {
        let mut data = Vec::new();

        if self.dirty {
            self.sorted_keys = self.map.keys().copied().collect();
            self.sorted_keys.sort_by(|a, b| {
                a.font_id
                    .cmp(&b.font_id)
                    .then(a.glyph_id.cmp(&b.glyph_id))
                    .then(a.font_size_bits.cmp(&b.font_size_bits))
            });
            self.dirty = false;
        }

        for key in &self.sorted_keys {
            let Some(entry) = self.map.get(key) else {
                continue;
            };
            if !self.is_alive(entry) {
                continue;
            }
            let base = entry.gpu_offset;
            let bd = &entry.band_data;
            let h_count = bd.h_count;
            let v_count = bd.v_count;
            let num_bands = h_count + v_count;
            let num_curves = entry.curves.len() as u32;

            let band_header_base = base + HEADER_WORDS;
            let mut total_refs = 0u32;
            for bi in 0..h_count as usize {
                total_refs += bd.h_bands[bi].indices.len() as u32;
            }
            for bi in 0..v_count as usize {
                total_refs += bd.v_bands[bi].indices.len() as u32;
            }
            debug_assert!(
                total_refs <= 0xFFFF,
                "ref_off overflow in slug glyph header"
            );
            let ref_base = band_header_base + num_bands;
            let curve_base = ref_base + total_refs;
            let block_end = curve_base + num_curves * 6;

            if data.len() < block_end as usize {
                data.resize(block_end as usize, 0);
            }

            // Header
            set(
                &mut data,
                base,
                (h_count & 0xFF) | ((v_count & 0xFF) << 8) | (num_curves << 16),
            );
            set(&mut data, base + 1, bd.band_scale[0].to_bits());
            set(&mut data, base + 2, bd.band_scale[1].to_bits());
            set(&mut data, base + 3, bd.band_offset[0].to_bits());
            set(&mut data, base + 4, bd.band_offset[1].to_bits());
            set(&mut data, base + 5, bd.bounds[0].to_bits());
            set(&mut data, base + 6, bd.bounds[1].to_bits());
            set(&mut data, base + 7, bd.bounds[2].to_bits());
            set(&mut data, base + 8, bd.bounds[3].to_bits());
            set(&mut data, base + 9, h_count | (v_count << 16));
            set(&mut data, base + 10, entry.font_size_bits);
            set(&mut data, base + 11, curve_base);

            // Band headers + refs (all NUM_BANDS bands written to match shader scale)
            let mut ref_off = 0u32;
            for bi in 0..h_count as usize {
                let count = bd.h_bands[bi].indices.len() as u32;
                set(
                    &mut data,
                    band_header_base + bi as u32,
                    (count & 0xFFFF) | (ref_off << 16),
                );
                for &ci in &bd.h_bands[bi].indices {
                    set(&mut data, ref_base + ref_off, ci);
                    ref_off += 1;
                }
            }
            for bi in 0..v_count as usize {
                let count = bd.v_bands[bi].indices.len() as u32;
                set(
                    &mut data,
                    band_header_base + h_count + bi as u32,
                    (count & 0xFFFF) | (ref_off << 16),
                );
                for &ci in &bd.v_bands[bi].indices {
                    set(&mut data, ref_base + ref_off, ci);
                    ref_off += 1;
                }
            }

            // Curve data
            for (ci, curve) in entry.curves.iter().enumerate() {
                let cb = curve_base + ci as u32 * 6;
                set(&mut data, cb, curve.p1[0].to_bits());
                set(&mut data, cb + 1, curve.p1[1].to_bits());
                set(&mut data, cb + 2, curve.p2[0].to_bits());
                set(&mut data, cb + 3, curve.p2[1].to_bits());
                set(&mut data, cb + 4, curve.p3[0].to_bits());
                set(&mut data, cb + 5, curve.p3[1].to_bits());
            }
        }

        data
    }

    /// Evict and compact. Called from next_frame() before scene traversal,
    /// so offsets are stable during vertex data capture.
    pub fn evict_stale(&mut self) {
        let frame = self.frame;
        let prev_len = self.map.len();
        self.map
            .retain(|_, e| frame.wrapping_sub(e.last_used) < EVICT_FRAMES);
        if self.map.is_empty() {
            self.next_offset = 0;
            self.dirty = true;
        } else if self.map.len() != prev_len {
            let mut keys: Vec<CacheKey> = self.map.keys().copied().collect();
            keys.sort_by(|a, b| {
                a.font_id
                    .cmp(&b.font_id)
                    .then(a.glyph_id.cmp(&b.glyph_id))
                    .then(a.font_size_bits.cmp(&b.font_size_bits))
            });
            let mut offset = 0u32;
            for key in &keys {
                let entry = self.map.get_mut(key).unwrap();
                entry.gpu_offset = offset;
                offset += Self::compute_block_size(&entry.curves, &entry.band_data);
            }
            self.next_offset = offset;
            self.dirty = true;
            self.generation = self.generation.wrapping_add(1);
        }
        // Shrink check: if we use less than a quarter of next_offset, cap it.
        if self.next_offset > MIN_STORAGE && self.next_offset > MIN_STORAGE * SHRINK_FACTOR as u32 {
            let active = self.map.iter().filter(|(_, e)| self.is_alive(e)).count() as u32;
            if self.next_offset > active * SHRINK_FACTOR as u32 * 32 {
                self.next_offset = self.next_offset.max(MIN_STORAGE);
            }
        }
    }

    pub fn iter_used(&self) -> impl Iterator<Item = (&CacheKey, &CachedGlyphSlug)> {
        self.map.iter().filter(move |(_, e)| self.is_alive(e))
    }

    pub fn storage_size(&self) -> u32 {
        let mut max = 0u32;
        for (_, entry) in &self.map {
            if self.is_alive(entry) {
                let end =
                    entry.gpu_offset + Self::compute_block_size(&entry.curves, &entry.band_data);
                if end > max {
                    max = end;
                }
            }
        }
        max
    }
}

#[inline]
fn set(data: &mut Vec<u32>, idx: u32, val: u32) {
    let i = idx as usize;
    if i >= data.len() {
        data.resize(i + 1, 0);
    }
    data[i] = val;
}
