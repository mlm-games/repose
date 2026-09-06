use font_awl::FontProvider;
use once_cell::sync::OnceCell;
use rapidhash::{HashMapExt, RapidHashMap, fast::RapidHasher};
use skrifa::MetadataProvider;
use skrifa::outline::OutlinePen;

pub mod fallback;
pub mod fallback_data;
pub mod unresolved;

use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::Mutex,
};
use unicode_segmentation::UnicodeSegmentation;

static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
static FONT_GENERATION: AtomicU64 = AtomicU64::new(0);
static FALLBACK_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn begin_frame() {
    FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
}

pub fn current_frame() -> u64 {
    FRAME_COUNTER.load(Ordering::Relaxed)
}

pub fn font_generation() -> u64 {
    FONT_GENERATION.load(Ordering::Relaxed)
}

pub fn take_fallback_dirty() -> bool {
    // NOTE: concurrent store(true) either before or after swap will be preserved
    // If it happens after swap, next call will return true. So swap is fine, keep simple but use SeqCst to ensure ordering
    FALLBACK_DIRTY.swap(false, Ordering::SeqCst)
}

const GLYPH_CACHE_CAP: usize = 4096;
const WRAP_CACHE_CAP: usize = 1024;
const ELLIP_CACHE_CAP: usize = 2048;

static METRICS_LRU: OnceCell<Mutex<Lru<(u64, u32, u64, u16, u8, i32, u64), TextMetrics>>> =
    OnceCell::new();
fn metrics_cache() -> &'static Mutex<Lru<(u64, u32, u64, u16, u8, i32, u64), TextMetrics>> {
    METRICS_LRU.get_or_init(|| Mutex::new(Lru::new(4096)))
}

struct Lru<K, V> {
    map: RapidHashMap<K, V>,
    ticks: RapidHashMap<K, u64>,
    order: VecDeque<K>,
    cap: usize,
    tick_counter: u64,
}
impl<K: std::hash::Hash + Eq + Clone, V> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            map: RapidHashMap::new(),
            ticks: RapidHashMap::new(),
            order: VecDeque::new(),
            cap,
            tick_counter: 0,
        }
    }
    fn get(&mut self, k: &K) -> Option<&V> {
        if self.map.contains_key(k) {
            self.tick_counter = self.tick_counter.wrapping_add(1);
            self.ticks.insert(k.clone(), self.tick_counter);
            // For correctness with existing clear(), we keep order in sync via tick map.
            // To keep order VecDeque consistent without O(n), we push new entry and skip stale on pop
            self.order.push_back(k.clone());
            if self.order.len() > self.cap * 3 {
                let mut pairs: Vec<(K, u64)> = self
                    .ticks
                    .iter()
                    .map(|(kk, tt)| (kk.clone(), *tt))
                    .collect();
                pairs.sort_by_key(|(_, t)| *t);
                self.order.clear();
                for (kk, _) in pairs {
                    self.order.push_back(kk);
                }
            }
        }
        self.map.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        self.tick_counter = self.tick_counter.wrapping_add(1);
        let is_new = !self.map.contains_key(&k);
        self.map.insert(k.clone(), v);
        self.ticks.insert(k.clone(), self.tick_counter);
        if is_new {
            self.order.push_back(k.clone());
        } else {
            self.order.push_back(k);
        }
        while self.map.len() > self.cap {
            let victim = {
                let mut min_key: Option<K> = None;
                let mut min_tick = u64::MAX;
                for (kk, tt) in &self.ticks {
                    if *tt < min_tick {
                        min_tick = *tt;
                        min_key = Some(kk.clone());
                    }
                }
                min_key
            };
            if let Some(victim) = victim {
                self.map.remove(&victim);
                self.ticks.remove(&victim);
                if let Some(pos) = self.order.iter().position(|x| x == &victim) {
                    self.order.remove(pos);
                }
            } else {
                break;
            }
        }
        if self.order.len() > self.cap * 2 {
            let mut seen = std::collections::HashSet::new();
            let mut compacted = VecDeque::new();
            // Keep only last occurrence per key, in tick order
            let mut pairs: Vec<(K, u64)> = self
                .ticks
                .iter()
                .map(|(kk, tt)| (kk.clone(), *tt))
                .collect();
            pairs.sort_by_key(|(_, t)| *t);
            for (kk, _) in pairs {
                if seen.insert(kk.clone()) {
                    compacted.push_back(kk);
                }
            }
            self.order = compacted;
        }
    }
    fn clear_both(&mut self) {
        self.map.clear();
        self.ticks.clear();
        self.order.clear();
        self.tick_counter = 0;
    }
}

static WRAP_LRU: OnceCell<
    Mutex<Lru<(u64, u32, u32, u16, bool, u16, u8, i32, u64), (Vec<String>, bool)>>,
> = OnceCell::new();

static WRAP_RANGES_LRU: OnceCell<
    Mutex<Lru<(u64, u32, u32, u16, bool, u16, u8, i32, u64), (Vec<(usize, usize)>, bool)>>,
> = OnceCell::new();

static ELLIP_LRU: OnceCell<Mutex<Lru<(u64, u32, u32, u16, u8, i32, u64), String>>> =
    OnceCell::new();

fn wrap_cache()
-> &'static Mutex<Lru<(u64, u32, u32, u16, bool, u16, u8, i32, u64), (Vec<String>, bool)>> {
    WRAP_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn wrap_ranges_cache()
-> &'static Mutex<Lru<(u64, u32, u32, u16, bool, u16, u8, i32, u64), (Vec<(usize, usize)>, bool)>> {
    WRAP_RANGES_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn ellip_cache() -> &'static Mutex<Lru<(u64, u32, u32, u16, u8, i32, u64), String>> {
    ELLIP_LRU.get_or_init(|| Mutex::new(Lru::new(ELLIP_CACHE_CAP)))
}

fn fast_hash(s: &str) -> u64 {
    let mut h = RapidHasher::default();
    s.len().hash(&mut h);
    s.hash(&mut h);
    h.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey(pub u64);

/// Cache key for the renderer's glyph slug cache -> uniquely identifies a
/// specific glyph in a specific font face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub font_id: u64,
    pub glyph_id: u16,
    pub font_size_bits: u32,
}

/// Vector path command for glyph outlines.
#[derive(Clone, Debug)]
pub enum Command {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

pub struct ShapedGlyph {
    pub key: GlyphKey,
    pub px: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

pub use swash::scale::image::Content as SwashContent;

pub struct GlyphBitmap {
    pub key: GlyphKey,
    pub w: u32,
    pub h: u32,
    pub content: SwashContent,
    pub data: Vec<u8>,
}

struct FontRecord {
    id: u64,
    data: parley::FontData,
    data_bytes: Vec<u8>,
}

struct Engine {
    font_cx: parley::FontContext,
    layout_cx: parley::LayoutContext<()>,
    swash_cx: swash::scale::ScaleContext,
    key_map: HashMap<GlyphKey, (u64, u16)>,
    font_registry: Vec<FontRecord>,
    next_font_id: u64,
    /// Cache of rendered glyphs keyed by (font_id, glyph_id, font_size_bits).
    /// Contains (width, height, left, top, content, data).
    glyph_cache:
        HashMap<(u64, u16, u32), (u32, u32, i32, i32, swash::scale::image::Content, Vec<u8>)>,
}

impl Engine {
    fn ensure_font(&mut self, fd: &parley::FontData) -> u64 {
        if let Some(existing) = self.font_registry.iter().find(|r| r.data == *fd) {
            log::debug!(
                "[font] reuse id={} len={}",
                existing.id,
                fd.data.as_ref().len()
            );
            return existing.id;
        }
        let id = self.next_font_id;
        self.next_font_id += 1;
        let bytes = fd.data.as_ref().to_vec();
        log::debug!("[font] register id={} len={}", id, bytes.len());
        self.font_registry.push(FontRecord {
            id,
            data: fd.clone(),
            data_bytes: bytes,
        });
        id
    }

    fn trim_glyph_cache(&mut self) {
        if self.glyph_cache.len() > GLYPH_CACHE_CAP {
            let to_remove = self.glyph_cache.len() - GLYPH_CACHE_CAP;
            let keys: Vec<_> = self.glyph_cache.keys().take(to_remove).copied().collect();
            for k in keys {
                self.glyph_cache.remove(&k);
            }
        }
    }

    fn raster_placement(
        &mut self,
        font_id: u64,
        glyph_id: u16,
        px: f32,
    ) -> Option<(f32, f32, f32, f32)> {
        use swash::scale::{Render, Source, StrikeWith};
        let cache_key = (font_id, glyph_id, px.to_bits());
        if let Some(cached) = self.glyph_cache.get(&cache_key) {
            log::debug!(
                "[raster_placement] HIT fid={} gid={} px={} => {}x{} {}x{}",
                font_id,
                glyph_id,
                px,
                cached.0,
                cached.1,
                cached.2,
                cached.3
            );
            return Some((
                cached.0 as f32,
                cached.1 as f32,
                cached.2 as f32,
                cached.3 as f32,
            ));
        }
        let data_bytes = self
            .font_registry
            .iter()
            .find(|r| r.id == font_id)?
            .data_bytes
            .clone();
        let font = swash::FontRef::from_index(&data_bytes, 0)?;
        let mut scaler = self.swash_cx.builder(font).size(px).hint(true).build();
        let image = Render::new(&[
            Source::Outline,
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
        ])
        .render(&mut scaler, glyph_id)?;
        log::debug!(
            "[raster_placement] MISS fid={} gid={} px={} => {}x{} {}x{}",
            font_id,
            glyph_id,
            px,
            image.placement.width,
            image.placement.height,
            image.placement.left,
            image.placement.top
        );
        self.glyph_cache.insert(
            cache_key,
            (
                image.placement.width,
                image.placement.height,
                image.placement.left,
                image.placement.top,
                image.content,
                image.data,
            ),
        );
        self.trim_glyph_cache();
        Some((
            image.placement.width as f32,
            image.placement.height as f32,
            image.placement.left as f32,
            image.placement.top as f32,
        ))
    }
}

static ENGINE: OnceCell<Mutex<Engine>> = OnceCell::new();

pub static FONT_PROVIDER: OnceCell<Mutex<font_awl::Provider>> = OnceCell::new();

fn init_engine_sync() -> Engine {
    let mut provider = font_awl::Provider::new();
    provider.load_bundled_fonts();
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(e) = provider.load_system_fonts_best_effort() {
        log::warn!("font-awl: failed to load system fonts: {e}");
    }

    let mut font_cx = provider.new_parley_context();
    // On wasm, bundled Symbols2 is NOT added to generic families by font-awl (bundled.rs only sets generic for OpenSans).
    // Without this, "sans-serif" text like Text("★") has no fallback to Symbols2 and renders as .notdef (gid 0).
    // Add Symbols2 to SansSerif generic at init so explicit fallback stacks and generic fallback both work.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(info) = font_cx.collection.family_by_name("Noto Sans Symbols 2") {
            let id = info.id();
            let mut existing: Vec<parley::fontique::FamilyId> = font_cx
                .collection
                .generic_families(parley::fontique::GenericFamily::SansSerif)
                .collect();
            if !existing.contains(&id) {
                existing.push(id);
                font_cx.collection.set_generic_families(
                    parley::fontique::GenericFamily::SansSerif,
                    existing.into_iter(),
                );
            }
        }
        // Also ensure Emoji generic exists (may be empty initially, but keep for layered fallback).
        // No-op if already set.
    }
    let layout_cx = parley::LayoutContext::new();

    static MATERIAL_SYMBOLS_TTF: &[u8] = include_bytes!("assets/MaterialSymbolsOutlined.ttf");
    let blob: parley::fontique::Blob<u8> = MATERIAL_SYMBOLS_TTF.to_vec().into();
    font_cx.collection.register_fonts(blob, None);

    let _ = FONT_PROVIDER.set(Mutex::new(provider));

    Engine {
        font_cx,
        layout_cx,
        swash_cx: swash::scale::ScaleContext::new(),
        key_map: HashMap::new(),
        font_registry: Vec::new(),
        next_font_id: 1,
        glyph_cache: HashMap::new(),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn init_fonts_wasm() {
    let mut provider = font_awl::Provider::new();
    provider.load_bundled_fonts();
    if let Err(e) = provider.load_web_fonts().await {
        log::warn!("font-awl: failed to load web fonts: {e}");
    }
    let _ = FONT_PROVIDER.set(Mutex::new(provider));

    if let Some(eng) = ENGINE.get() {
        let mut eng = eng.lock().unwrap();
        // Register web fonts into the existing engine's collection
        // by re-building font_cx from the updated provider
        if let Some(provider_lock) = FONT_PROVIDER.get() {
            let p = provider_lock.lock().unwrap();
            eng.font_cx = p.new_parley_context();
        }
    }
}

fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| Mutex::new(init_engine_sync()))
}

pub fn register_font_data(bytes: &[u8]) {
    let mut eng = engine().lock().unwrap();
    let blob: parley::fontique::Blob<u8> = bytes.to_vec().into();
    let families = eng.font_cx.collection.register_fonts(blob.clone(), None);
    // Detect family type: font_family_name fails for woff2 (skrifa needs decompressed), so fallback to registered family names
    let mut is_emoji = false;
    let mut is_symbols = false;
    if let Some(name) = font_family_name(bytes) {
        if name.starts_with("Noto Color Emoji") {
            is_emoji = true;
        } else if name.starts_with("Noto Sans Symbols") {
            is_symbols = true;
        }
    }
    if !is_emoji && !is_symbols {
        for (fid, _) in &families {
            if let Some(fname) = eng.font_cx.collection.family_name(*fid) {
                if fname.starts_with("Noto Color Emoji") {
                    is_emoji = true;
                }
                if fname.starts_with("Noto Sans Symbols") {
                    is_symbols = true;
                }
            } else if let Some(info) = eng.font_cx.collection.family(*fid) {
                let fname = info.name();
                if fname.starts_with("Noto Color Emoji") {
                    is_emoji = true;
                }
                if fname.starts_with("Noto Sans Symbols") {
                    is_symbols = true;
                }
            }
        }
    }
    if is_emoji {
        let ids: Vec<parley::fontique::FamilyId> = families.iter().map(|(fid, _)| *fid).collect();
        let mut existing: Vec<parley::fontique::FamilyId> = eng
            .font_cx
            .collection
            .generic_families(parley::fontique::GenericFamily::Emoji)
            .collect();
        for id in ids.clone() {
            if !existing.contains(&id) {
                existing.push(id);
            }
        }
        eng.font_cx
            .collection
            .set_generic_families(parley::fontique::GenericFamily::Emoji, existing.into_iter());
    } else if is_symbols {
        let ids: Vec<parley::fontique::FamilyId> = families.iter().map(|(fid, _)| *fid).collect();
        let mut existing: Vec<parley::fontique::FamilyId> = eng
            .font_cx
            .collection
            .generic_families(parley::fontique::GenericFamily::SansSerif)
            .collect();
        for id in ids.clone() {
            if !existing.contains(&id) {
                existing.push(id);
            }
        }
        eng.font_cx.collection.set_generic_families(
            parley::fontique::GenericFamily::SansSerif,
            existing.into_iter(),
        );
    }
    // Clear source cache so next layout re-resolves fonts (mirrors Compose invalidation)
    eng.font_cx.source_cache = parley::fontique::SourceCache::default();
    if let Some(provider_lock) = FONT_PROVIDER.get() {
        let mut p = provider_lock.lock().unwrap();
        let families2 = p.collection_mut().register_fonts(blob, None);
        // Mirror generic setup for provider's collection as well (used for new contexts)
        if is_emoji {
            let ids: Vec<parley::fontique::FamilyId> =
                families2.iter().map(|(fid, _)| *fid).collect();
            let mut existing: Vec<parley::fontique::FamilyId> = p
                .collection_mut()
                .generic_families(parley::fontique::GenericFamily::Emoji)
                .collect();
            for id in ids.clone() {
                if !existing.contains(&id) {
                    existing.push(id);
                }
            }
            p.collection_mut()
                .set_generic_families(parley::fontique::GenericFamily::Emoji, existing.into_iter());
        } else if is_symbols {
            let ids: Vec<parley::fontique::FamilyId> =
                families2.iter().map(|(fid, _)| *fid).collect();
            let mut existing: Vec<parley::fontique::FamilyId> = p
                .collection_mut()
                .generic_families(parley::fontique::GenericFamily::SansSerif)
                .collect();
            for id in ids.clone() {
                if !existing.contains(&id) {
                    existing.push(id);
                }
            }
            p.collection_mut().set_generic_families(
                parley::fontique::GenericFamily::SansSerif,
                existing.into_iter(),
            );
        }
    }
    // Invalidate caches so text with newly available glyphs will relayout
    clear_caches_for_fallback();
    // Notify unresolved registry (mirrors Compose: fontFamilyResolver.preload + onNewFontInstalled)
    #[cfg(target_arch = "wasm32")]
    {
        // Re-invalidate via registry to trigger ParagraphLayouter-style listeners
        // (wasm_fallback also does this; double-clear is safe)
        crate::unresolved::web_unresolved_registry().on_new_font_installed();
    }
}

pub(crate) fn clear_caches_for_fallback() {
    if let Some(c) = METRICS_LRU.get()
        && let Ok(mut g) = c.lock() {
            g.clear_both();
        }
    if let Some(c) = WRAP_LRU.get()
        && let Ok(mut g) = c.lock() {
            g.clear_both();
        }
    if let Some(c) = WRAP_RANGES_LRU.get()
        && let Ok(mut g) = c.lock() {
            g.clear_both();
        }
    if let Some(c) = ELLIP_LRU.get()
        && let Ok(mut g) = c.lock() {
            g.clear_both();
        }
    // Also bump frame counter to signal stale
    bump_frame_for_fallback();
}

pub(crate) fn bump_frame_for_fallback() {
    FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    FONT_GENERATION.fetch_add(1, Ordering::Relaxed);
    FALLBACK_DIRTY.store(true, Ordering::Relaxed);
}

#[cfg(target_arch = "wasm32")]
pub fn ensure_web_fallback_initialized() {
    crate::fallback::wasm_fallback::ensure_fallback_initialized();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ensure_web_fallback_initialized() {}

/// Load a font from a file path and register it into the global font system.
///
/// Returns an error if the file cannot be read.
pub fn load_font_file(path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    register_font_data(&bytes);
    Ok(())
}

/// Extract the family name from raw font bytes.
///
/// Tries the typographic family name first, falling back to the standard family name.
/// Returns `None` if the font data is invalid or contains no names.
pub fn font_family_name(bytes: &[u8]) -> Option<String> {
    use skrifa::string::StringId;
    let font = skrifa::FontRef::new(bytes).ok()?;
    font.localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
        .english_or_first()
        .map(|s| s.to_string())
        .or_else(|| {
            font.localized_strings(StringId::FAMILY_NAME)
                .english_or_first()
                .map(|s| s.to_string())
        })
}

fn key_from_pair(font_id: u64, glyph_id: u16) -> GlyphKey {
    let mut h = RapidHasher::default();
    font_id.hash(&mut h);
    glyph_id.hash(&mut h);
    GlyphKey(h.finish())
}

#[cfg(target_arch = "wasm32")]
fn collect_unresolved_codepoints(layout: &parley::Layout<()>, text: &str) -> Vec<u32> {
    use parley::layout::PositionedLayoutItem;
    let mut out = Vec::new();
    let mut total_glyphs: usize = 0;
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            // parley clusters already grouped by text; if glyph id==0 => missing
            for cluster in run.clusters() {
                let glyphs: Vec<_> = cluster.glyphs().collect();
                total_glyphs += glyphs.len();
                let has_missing = glyphs.iter().any(|g| g.id == 0);
                if has_missing {
                    let range = cluster.text_range();
                    // slice may be invalid if out of bounds? clamp
                    let end = range.end.min(text.len());
                    let start = range.start.min(end);
                    for ch in text[start..end].chars() {
                        out.push(ch as u32);
                    }
                    // Fallback: if text_range empty but still missing, push replacement
                    if range.start == range.end {
                        // try to guess from glyph? skip
                    }
                }
            }
        }
    }
    if out.is_empty() && !text.is_empty() && total_glyphs == 0 {
        if text.chars().any(|c| !c.is_whitespace()) {
            for ch in text.chars() {
                if !ch.is_whitespace() && ch != '\n' && ch != '\r' && ch != '\t' {
                    out.push(ch as u32);
                }
            }
        }
    }
    out
}

fn shape_line_inner(
    eng: &mut Engine,
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Vec<ShapedGlyph> {
    use parley::FontWeight;
    use parley::layout::PositionedLayoutItem;
    use parley::style::StyleProperty;

    let Engine {
        ref mut font_cx,
        ref mut layout_cx,
        ..
    } = *eng;
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(px));
    if line_height_ratio > 0.0 {
        builder.push_default(StyleProperty::LineHeight(
            parley::LineHeight::FontSizeRelative(line_height_ratio),
        ));
    }
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(
        font_weight as f32,
    )));
    builder.push_default(StyleProperty::FontStyle(match font_style {
        1 => parley::FontStyle::Italic,
        _ => parley::FontStyle::Normal,
    }));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));

    if let Some(settings) = font_variation_settings {
        builder.push_default(StyleProperty::FontVariations(
            parley::style::FontVariations::from(settings),
        ));
    }

    if let Some(family) = font_family {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let names: &[FontFamilyName] = match family {
                "monospace" => &[
                    FontFamilyName::named("JetBrains Mono"),
                    GenericFamily::Monospace.into(),
                ],
                "sans-serif" => &[
                    FontFamilyName::named("Open Sans"),
                    GenericFamily::SansSerif.into(),
                ],
                "emoji" => &[
                    FontFamilyName::named("Noto Color Emoji"),
                    GenericFamily::Emoji.into(),
                ],
                "serif" => &[GenericFamily::Serif.into()],
                "cursive" => &[GenericFamily::Cursive.into()],
                "fantasy" => &[GenericFamily::Fantasy.into()],
                "system-ui" => &[GenericFamily::SystemUi.into()],
                "math" => &[GenericFamily::Math.into()],
                _ => &[FontFamilyName::named(family)],
            };
            builder.push(names, 0..text.len());
        }
        #[cfg(target_arch = "wasm32")]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let names: &[FontFamilyName] = match family {
                "monospace" => &[
                    FontFamilyName::named("JetBrains Mono"),
                    GenericFamily::Monospace.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                    FontFamilyName::named("Noto Sans Symbols2"),
                    FontFamilyName::named("Noto Sans Symbols"),
                ],
                "sans-serif" => &[
                    FontFamilyName::named("Open Sans"),
                    GenericFamily::SansSerif.into(),
                    GenericFamily::Emoji.into(),
                    FontFamilyName::named("Noto Color Emoji"),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                    FontFamilyName::named("Noto Sans Symbols2"),
                    FontFamilyName::named("Noto Sans Symbols"),
                ],
                "emoji" => &[
                    FontFamilyName::named("Noto Color Emoji"),
                    GenericFamily::Emoji.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "serif" => &[
                    GenericFamily::Serif.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "cursive" => &[
                    GenericFamily::Cursive.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "fantasy" => &[
                    GenericFamily::Fantasy.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "system-ui" => &[
                    GenericFamily::SystemUi.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "math" => &[
                    GenericFamily::Math.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                _ => &[FontFamilyName::named(family)],
            };
            builder.push(names, 0..text.len());
        }
    } else {
        #[cfg(target_arch = "wasm32")]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let fallback: &[FontFamilyName] = &[
                GenericFamily::SansSerif.into(),
                GenericFamily::Emoji.into(),
                FontFamilyName::named("Noto Color Emoji"),
                FontFamilyName::named("Noto Sans Symbols 2"),
                FontFamilyName::named("Noto Sans Symbols2"),
                FontFamilyName::named("Noto Sans Symbols"),
            ];
            builder.push(fallback, 0..text.len());
        }
    }

    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(
        parley::Alignment::Start,
        parley::AlignmentOptions::default(),
    );

    #[cfg(target_arch = "wasm32")]
    {
        let unresolved = collect_unresolved_codepoints(&layout, text);
        // Filter out PUA (Material Symbols etc. E000-F8FF, F0000-FFFFD, 100000-10FFFD) - they are bundled via MaterialSymbolsOutlined.ttf, not Noto fallback
        let unresolved: Vec<u32> = unresolved
            .into_iter()
            .filter(|cp| {
                !((0xE000..=0xF8FF).contains(cp)
                    || (0xF0000..=0xFFFFD).contains(cp)
                    || (0x100000..=0x10FFFD).contains(cp))
            })
            .collect();
        if !unresolved.is_empty() {
            let reg = crate::unresolved::web_unresolved_registry();
            let is_new = unresolved.iter().any(|cp| !reg.contains(*cp));
            if is_new {
                crate::fallback::wasm_fallback::ensure_fallback_initialized();
                reg.add_unresolved_vec(unresolved);
            }
        }
    }

    let mut out: Vec<ShapedGlyph> = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let font_data = glyph_run.run().font();
            let fid = eng.ensure_font(font_data);
            log::debug!(
                "[shape] run: fid={} font_data_len={}",
                fid,
                font_data.data.as_ref().len()
            );
            for g in glyph_run.positioned_glyphs() {
                let gid = g.id as u16;
                let key = key_from_pair(fid, gid);
                eng.key_map.insert(key, (fid, gid));

                let (w, h, left, top) = eng
                    .raster_placement(fid, gid, px)
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));

                log::debug!(
                    "[shape] glyph: gid={} px={} x={:.1} y={:.1} advance={:.1} bitmap={}x{} {}x{}",
                    gid,
                    px,
                    g.x,
                    g.y,
                    g.advance,
                    w,
                    h,
                    left,
                    top,
                );

                out.push(ShapedGlyph {
                    key,
                    px,
                    x: g.x,
                    y: g.y,
                    w,
                    h,
                    bearing_x: left,
                    bearing_y: top,
                    advance: g.advance + letter_spacing,
                });
            }
        }
    }
    out
}

pub fn shape_line(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Vec<ShapedGlyph> {
    let mut eng = engine().lock().unwrap();
    shape_line_inner(
        &mut eng,
        text,
        px,
        line_height_ratio,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn rasterize(key: GlyphKey, px: f32) -> Option<GlyphBitmap> {
    use swash::scale::{Render, Source, StrikeWith};
    let mut eng = engine().lock().unwrap();
    let &(fid, gid) = eng.key_map.get(&key)?;
    let cache_key = (fid, gid, px.to_bits());
    if let Some(cached) = eng.glyph_cache.get(&cache_key) {
        log::debug!(
            "[rasterize] HIT fid={} gid={} px={} => {}x{}",
            fid,
            gid,
            px,
            cached.0,
            cached.1
        );
        return Some(GlyphBitmap {
            key,
            w: cached.0,
            h: cached.1,
            content: cached.4,
            data: cached.5.clone(),
        });
    }
    let data_bytes = eng
        .font_registry
        .iter()
        .find(|r| r.id == fid)?
        .data_bytes
        .clone();
    let font = swash::FontRef::from_index(&data_bytes, 0)?;
    let mut scaler = eng.swash_cx.builder(font).size(px).hint(true).build();
    let image = Render::new(&[
        Source::Outline,
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::ColorOutline(0),
    ])
    .render(&mut scaler, gid)?;
    log::debug!(
        "[rasterize] MISS fid={} gid={} px={} => {}x{}",
        fid,
        gid,
        px,
        image.placement.width,
        image.placement.height
    );
    let bitmap = GlyphBitmap {
        key,
        w: image.placement.width,
        h: image.placement.height,
        content: image.content,
        data: image.data,
    };
    eng.glyph_cache.insert(
        cache_key,
        (
            bitmap.w,
            bitmap.h,
            image.placement.left,
            image.placement.top,
            bitmap.content,
            bitmap.data.clone(),
        ),
    );
    eng.trim_glyph_cache();
    Some(bitmap)
}

pub fn lookup_cache_key(key: GlyphKey, px: f32) -> Option<CacheKey> {
    let eng = engine().lock().unwrap();
    let &(fid, gid) = eng.key_map.get(&key)?;
    Some(CacheKey {
        font_id: fid,
        glyph_id: gid,
        font_size_bits: px.to_bits(),
    })
}

fn extract_outlines_for(data_bytes: &[u8], glyph_id: u16) -> Option<Box<[Command]>> {
    let font = skrifa::FontRef::new(data_bytes).ok()?;
    let mut pen = OutlinePenCollector(Vec::new());
    font.outline_glyphs()
        .get(skrifa::GlyphId::new(glyph_id as u32))?
        .draw(skrifa::instance::Size::new(1.0), &mut pen)
        .ok()?;
    Some(pen.0.into_boxed_slice())
}

pub fn extract_outline_commands(cache_key: CacheKey) -> Option<Box<[Command]>> {
    let eng = engine().lock().unwrap();
    let record = eng
        .font_registry
        .iter()
        .find(|r| r.id == cache_key.font_id)?;
    extract_outlines_for(&record.data_bytes, cache_key.glyph_id)
}

pub fn lookup_and_extract_outline(key: GlyphKey, px: f32) -> Option<(CacheKey, Box<[Command]>)> {
    let eng = engine().lock().unwrap();
    let &(fid, gid) = eng.key_map.get(&key)?;
    let record = eng.font_registry.iter().find(|r| r.id == fid)?;
    let ck = CacheKey {
        font_id: fid,
        glyph_id: gid,
        font_size_bits: px.to_bits(),
    };
    let cmds = extract_outlines_for(&record.data_bytes, gid)?;
    Some((ck, cmds))
}

struct OutlinePenCollector(Vec<Command>);

impl OutlinePen for OutlinePenCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::MoveTo(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::LineTo(x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(Command::QuadTo(cx0, cy0, x, y));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Command::CurveTo(cx0, cy0, cx1, cy1, x, y));
    }
    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}

#[derive(Clone)]
pub struct TextMetrics {
    pub positions: Vec<f32>,
    pub byte_offsets: Vec<usize>,
}

pub fn metrics_for_textfield(
    text: &str,
    px: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> TextMetrics {
    let family_hash = font_family.map(fast_hash).unwrap_or(0);
    let fvs_hash = font_variation_settings.map(fast_hash).unwrap_or(0);
    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        family_hash,
        font_weight,
        font_style,
        (letter_spacing * 100.0) as i32,
        fvs_hash,
    );
    if let Some(m) = metrics_cache().lock().unwrap().get(&key).cloned() {
        return m;
    }
    let mut eng = engine().lock().unwrap();

    use parley::FontWeight;
    use parley::style::StyleProperty;

    let Engine {
        ref mut font_cx,
        ref mut layout_cx,
        ..
    } = *eng;
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(px));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(
        font_weight as f32,
    )));
    builder.push_default(StyleProperty::FontStyle(match font_style {
        1 => parley::FontStyle::Italic,
        _ => parley::FontStyle::Normal,
    }));
    builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
    if let Some(settings) = font_variation_settings {
        builder.push_default(StyleProperty::FontVariations(
            parley::style::FontVariations::from(settings),
        ));
    }
    if let Some(family) = font_family {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let names: &[FontFamilyName] = match family {
                "monospace" => &[
                    FontFamilyName::named("JetBrains Mono"),
                    GenericFamily::Monospace.into(),
                ],
                "sans-serif" => &[
                    FontFamilyName::named("Open Sans"),
                    GenericFamily::SansSerif.into(),
                ],
                "emoji" => &[
                    FontFamilyName::named("Noto Color Emoji"),
                    GenericFamily::Emoji.into(),
                ],
                "serif" => &[GenericFamily::Serif.into()],
                "cursive" => &[GenericFamily::Cursive.into()],
                "fantasy" => &[GenericFamily::Fantasy.into()],
                "system-ui" => &[GenericFamily::SystemUi.into()],
                "math" => &[GenericFamily::Math.into()],
                _ => &[FontFamilyName::named(family)],
            };
            builder.push(names, 0..text.len());
        }
        #[cfg(target_arch = "wasm32")]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let names: &[FontFamilyName] = match family {
                "monospace" => &[
                    FontFamilyName::named("JetBrains Mono"),
                    GenericFamily::Monospace.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                    FontFamilyName::named("Noto Sans Symbols2"),
                    FontFamilyName::named("Noto Sans Symbols"),
                ],
                "sans-serif" => &[
                    FontFamilyName::named("Open Sans"),
                    GenericFamily::SansSerif.into(),
                    GenericFamily::Emoji.into(),
                    FontFamilyName::named("Noto Color Emoji"),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                    FontFamilyName::named("Noto Sans Symbols2"),
                    FontFamilyName::named("Noto Sans Symbols"),
                ],
                "emoji" => &[
                    FontFamilyName::named("Noto Color Emoji"),
                    GenericFamily::Emoji.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "serif" => &[
                    GenericFamily::Serif.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "cursive" => &[
                    GenericFamily::Cursive.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "fantasy" => &[
                    GenericFamily::Fantasy.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "system-ui" => &[
                    GenericFamily::SystemUi.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                "math" => &[
                    GenericFamily::Math.into(),
                    GenericFamily::SansSerif.into(),
                    FontFamilyName::named("Noto Sans Symbols 2"),
                ],
                _ => &[FontFamilyName::named(family)],
            };
            builder.push(names, 0..text.len());
        }
    } else {
        #[cfg(target_arch = "wasm32")]
        {
            use parley::style::{FontFamilyName, GenericFamily};
            let fallback: &[FontFamilyName] = &[
                GenericFamily::SansSerif.into(),
                GenericFamily::Emoji.into(),
                FontFamilyName::named("Noto Color Emoji"),
                FontFamilyName::named("Noto Sans Symbols 2"),
                FontFamilyName::named("Noto Sans Symbols2"),
                FontFamilyName::named("Noto Sans Symbols"),
            ];
            builder.push(fallback, 0..text.len());
        }
    }

    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(
        parley::Alignment::Start,
        parley::AlignmentOptions::default(),
    );

    #[cfg(target_arch = "wasm32")]
    {
        let unresolved = collect_unresolved_codepoints(&layout, text);
        let unresolved: Vec<u32> = unresolved
            .into_iter()
            .filter(|cp| {
                !((0xE000..=0xF8FF).contains(cp)
                    || (0xF0000..=0xFFFFD).contains(cp)
                    || (0x100000..=0x10FFFD).contains(cp))
            })
            .collect();
        if !unresolved.is_empty() {
            let reg = crate::unresolved::web_unresolved_registry();
            let is_new = unresolved.iter().any(|cp| !reg.contains(*cp));
            if is_new {
                crate::fallback::wasm_fallback::ensure_fallback_initialized();
                reg.add_unresolved_vec(unresolved);
            }
        }
    }

    let mut edges: Vec<(usize, f32)> = Vec::new();
    let mut last_x = 0.0f32;
    let mut glyph_idx = 0usize;
    for line in layout.lines() {
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run_offset = glyph_run.offset();
            let run = glyph_run.run();
            let mut cluster_offset = run_offset;
            for cluster in run.clusters() {
                let range = cluster.text_range();
                for g in cluster.glyphs() {
                    let shift = glyph_idx as f32 * letter_spacing;
                    let x_pos = cluster_offset + g.x;
                    let right = x_pos + shift + g.advance + letter_spacing;
                    last_x = right.max(last_x);
                    edges.push((range.end, right));
                    glyph_idx += 1;
                    cluster_offset += g.advance;
                }
            }
        }
    }
    if edges.last().map(|e| e.0) != Some(text.len()) {
        edges.push((text.len(), last_x));
    }

    let mut positions = Vec::with_capacity(text.graphemes(true).count() + 1);
    let mut byte_offsets = Vec::with_capacity(positions.capacity());
    positions.push(0.0);
    byte_offsets.push(0);
    let mut last_byte = 0usize;
    for (b, _) in text.grapheme_indices(true) {
        positions
            .push(positions.last().copied().unwrap_or(0.0) + width_between(&edges, last_byte, b));
        byte_offsets.push(b);
        last_byte = b;
    }
    if *byte_offsets.last().unwrap_or(&0) != text.len() {
        positions.push(
            positions.last().copied().unwrap_or(0.0) + width_between(&edges, last_byte, text.len()),
        );
        byte_offsets.push(text.len());
    }
    let m = TextMetrics {
        positions,
        byte_offsets,
    };
    metrics_cache().lock().unwrap().put(key, m.clone());
    m
}

fn width_between(edges: &[(usize, f32)], start_b: usize, end_b: usize) -> f32 {
    let x0 = lookup_right(edges, start_b);
    let x1 = lookup_right(edges, end_b);
    (x1 - x0).max(0.0)
}
fn lookup_right(edges: &[(usize, f32)], b: usize) -> f32 {
    match edges.binary_search_by_key(&b, |e| e.0) {
        Ok(i) => edges[i].1,
        Err(i) => {
            if i == 0 {
                0.0
            } else {
                edges[i - 1].1
            }
        }
    }
}

pub fn wrap_lines(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<String>, bool) {
    if text.is_empty() || max_width <= 0.0 {
        return (vec![String::new()], false);
    }
    if !soft_wrap {
        return (vec![text.to_string()], false);
    }

    let max_lines_key: u16 = match max_lines {
        None => 0,
        Some(n) => {
            let n = n.min(u16::MAX as usize - 1) as u16;
            n.saturating_add(1)
        }
    };
    let fvs_hash = font_variation_settings.map(fast_hash).unwrap_or(0);
    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        (max_width * 100.0) as u32,
        max_lines_key,
        soft_wrap,
        font_weight,
        font_style,
        (letter_spacing * 100.0) as i32,
        fvs_hash,
    );
    if let Some(h) = wrap_cache().lock().unwrap().get(&key).cloned() {
        return h;
    }

    let m = metrics_for_textfield(
        text,
        px,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    if let Some(&last) = m.positions.last()
        && last <= max_width + 0.5
    {
        return (vec![text.to_string()], false);
    }

    let width_of = |start_b: usize, end_b: usize| -> f32 {
        let i0 = match m.byte_offsets.binary_search(&start_b) {
            Ok(i) | Err(i) => i,
        };
        let i1 = match m.byte_offsets.binary_search(&end_b) {
            Ok(i) | Err(i) => i,
        };
        (m.positions.get(i1).copied().unwrap_or(0.0) - m.positions.get(i0).copied().unwrap_or(0.0))
            .max(0.0)
    };

    let mut out: Vec<String> = Vec::new();
    let mut truncated = false;

    let mut line_start = 0usize;
    let mut best_break = line_start;

    for tok in text.split_word_bounds() {
        let tok_start = best_break;
        let tok_end = tok_start + tok.len();
        let w = width_of(line_start, tok_end);

        if w <= max_width + 0.5 {
            best_break = tok_end;
            continue;
        }

        if best_break > line_start {
            out.push(text[line_start..best_break].trim_end().to_string());
            line_start = best_break;
        } else {
            let mut cut = tok_start;
            for g in tok.grapheme_indices(true) {
                let next = tok_start + g.0 + g.1.len();
                if width_of(line_start, next) <= max_width + 0.5 {
                    cut = next;
                } else {
                    break;
                }
            }
            if cut == line_start
                && let Some((ofs, grapheme)) = tok.grapheme_indices(true).next()
            {
                cut = tok_start + ofs + grapheme.len();
            }
            out.push(text[line_start..cut].to_string());
            line_start = cut;
        }

        if let Some(ml) = max_lines
            && out.len() >= ml
        {
            truncated = true;
            line_start = line_start.min(text.len());
            break;
        }

        best_break = line_start;

        if line_start < tok_end && width_of(line_start, tok_end) <= max_width + 0.5 {
            best_break = tok_end;
        }
    }

    if line_start < text.len() && max_lines.is_none_or(|ml| out.len() < ml) {
        out.push(text[line_start..].trim_end().to_string());
    }

    let res = (out, truncated);

    wrap_cache().lock().unwrap().put(key, res.clone());
    res
}

pub fn wrap_line_ranges(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<(usize, usize)>, bool) {
    if text.is_empty() || max_width <= 0.0 {
        return (vec![(0, 0)], false);
    }
    if !soft_wrap {
        let mut out = Vec::new();
        let mut start = 0usize;
        for (i, ch) in text.char_indices() {
            if ch == '\n' {
                out.push((start, i));
                start = i + 1;
            }
        }
        out.push((start, text.len()));
        return (out, false);
    }

    let max_lines_key: u16 = match max_lines {
        None => 0,
        Some(n) => {
            let n = n.min(u16::MAX as usize - 1) as u16;
            n.saturating_add(1)
        }
    };
    let fvs_hash = font_variation_settings.map(fast_hash).unwrap_or(0);
    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        (max_width * 100.0) as u32,
        max_lines_key,
        soft_wrap,
        font_weight,
        font_style,
        (letter_spacing * 100.0) as i32,
        fvs_hash,
    );
    if let Some(v) = wrap_ranges_cache().lock().unwrap().get(&key).cloned() {
        return v;
    }

    let m = metrics_for_textfield(
        text,
        px,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );

    let width_of = |start_b: usize, end_b: usize| -> f32 {
        let i0 = match m.byte_offsets.binary_search(&start_b) {
            Ok(i) | Err(i) => i,
        };
        let i1 = match m.byte_offsets.binary_search(&end_b) {
            Ok(i) | Err(i) => i,
        };
        (m.positions.get(i1).copied().unwrap_or(0.0) - m.positions.get(i0).copied().unwrap_or(0.0))
            .max(0.0)
    };

    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut truncated = false;

    let mut line0_start = 0usize;
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            let (mut ranges, tr) = wrap_one_hard_line_ranges(
                text,
                line0_start,
                i,
                max_width,
                max_lines.map(|ml| ml.saturating_sub(out.len())),
                &width_of,
            );
            out.append(&mut ranges);
            if tr {
                truncated = true;
                break;
            }
            line0_start = i + 1;

            if let Some(ml) = max_lines
                && out.len() >= ml
            {
                truncated = true;
                break;
            }
        }
    }
    if !truncated {
        let (mut ranges, tr) = wrap_one_hard_line_ranges(
            text,
            line0_start,
            text.len(),
            max_width,
            max_lines.map(|ml| ml.saturating_sub(out.len())),
            &width_of,
        );
        out.append(&mut ranges);
        truncated = tr;
    }

    if out.is_empty() {
        out.push((0, 0));
    }

    let res = (out, truncated);
    wrap_ranges_cache().lock().unwrap().put(key, res.clone());
    res
}

fn wrap_one_hard_line_ranges(
    text: &str,
    start: usize,
    end: usize,
    max_width: f32,
    max_lines: Option<usize>,
    width_of: &dyn Fn(usize, usize) -> f32,
) -> (Vec<(usize, usize)>, bool) {
    let mut out = Vec::new();
    let mut t = false;

    if start >= end {
        out.push((start, start));
        return (out, false);
    }

    if width_of(start, end) <= max_width + 0.5 {
        out.push((start, end));
        return (out, false);
    }

    let mut line_start = start;
    let mut best_break = line_start;
    let mut unconsumed_start = start;

    for tok in text[line_start..end].split_word_bounds() {
        let tok_abs_start = unconsumed_start;
        let tok_abs_end = tok_abs_start + tok.len();
        unconsumed_start = tok_abs_end;

        let w = width_of(line_start, tok_abs_end);
        if w <= max_width + 0.5 {
            best_break = tok_abs_end;
            continue;
        }

        if best_break > line_start {
            out.push((line_start, best_break));
            line_start = best_break;
        } else {
            let mut cut = tok_abs_start;
            for (ofs, g) in tok.grapheme_indices(true) {
                let next = tok_abs_start + ofs + g.len();
                if width_of(line_start, next) <= max_width + 0.5 {
                    cut = next;
                } else {
                    break;
                }
            }
            if cut == line_start
                && let Some((ofs, gr)) = tok.grapheme_indices(true).next()
            {
                cut = tok_abs_start + ofs + gr.len();
            }
            out.push((line_start, cut));
            line_start = cut;
        }

        if let Some(ml) = max_lines
            && out.len() >= ml
        {
            t = true;
            break;
        }

        best_break = line_start;
    }

    if !t && line_start < end && max_lines.is_none_or(|ml| out.len() < ml) {
        out.push((line_start, end));
    }

    (out, t)
}

pub fn ellipsize_line(
    text: &str,
    px: f32,
    max_width: f32,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    let fvs_hash = font_variation_settings.map(fast_hash).unwrap_or(0);
    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        (max_width * 100.0) as u32,
        font_weight,
        font_style,
        (letter_spacing * 100.0) as i32,
        fvs_hash,
    );
    if let Some(s) = ellip_cache().lock().unwrap().get(&key).cloned() {
        return s;
    }
    let m = metrics_for_textfield(
        text,
        px,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    if let Some(&last) = m.positions.last()
        && last <= max_width + 0.5
    {
        return text.to_string();
    }
    let _el = "…";
    let e_w = ellipsis_width(px, letter_spacing);
    if e_w >= max_width {
        return String::new();
    }
    let mut cut_i = 0usize;
    for i in 0..m.positions.len() {
        if m.positions[i] + e_w <= max_width {
            cut_i = i;
        } else {
            break;
        }
    }
    let byte = m
        .byte_offsets
        .get(cut_i)
        .copied()
        .unwrap_or(0)
        .min(text.len());
    let mut out = String::with_capacity(byte + 3);
    out.push_str(&text[..byte]);
    out.push('…');

    let s = out;
    ellip_cache().lock().unwrap().put(key, s.clone());

    s
}

fn ellipsis_width(px: f32, letter_spacing: f32) -> f32 {
    static ELLIP_W_LRU: OnceCell<Mutex<Lru<(u32, i32), f32>>> = OnceCell::new();
    let cache = ELLIP_W_LRU.get_or_init(|| Mutex::new(Lru::new(64)));
    let key = ((px * 100.0) as u32, (letter_spacing * 100.0) as i32);
    if let Some(w) = cache.lock().unwrap().get(&key).copied() {
        return w;
    }
    let w = if let Some(g) =
        crate::shape_line("…", px, px, None, 400, 0, letter_spacing, None).last()
    {
        g.x + g.advance
    } else {
        0.0
    };
    cache.lock().unwrap().put(key, w);
    w
}
