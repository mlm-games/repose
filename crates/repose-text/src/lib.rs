use font_awl::FontProvider;
use rapidhash::{HashMapExt, RapidHashMap, fast::RapidHasher};
use skrifa::MetadataProvider;
use skrifa::outline::OutlinePen;
use std::sync::OnceLock;

pub mod fallback;
pub mod fallback_data;
pub mod unresolved;

use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
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

#[derive(Clone, Hash, Eq, PartialEq)]
struct MetricsCacheKey {
    text: String,
    px_bits: u32,
    family: Option<String>,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<String>,
    generation: u64,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct WrapCacheKey {
    text: String,
    px_bits: u32,
    max_width_bits: u32,
    max_lines: Option<usize>,
    family: Option<String>,
    soft_wrap: bool,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<String>,
    generation: u64,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct EllipCacheKey {
    text: String,
    px_bits: u32,
    max_width_bits: u32,
    family: Option<String>,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<String>,
    generation: u64,
}

static METRICS_LRU: OnceLock<Mutex<Lru<MetricsCacheKey, TextMetrics>>> = OnceLock::new();
fn metrics_cache() -> &'static Mutex<Lru<MetricsCacheKey, TextMetrics>> {
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

static WRAP_LRU: OnceLock<Mutex<Lru<WrapCacheKey, (Vec<String>, bool)>>> = OnceLock::new();

static WRAP_RANGES_LRU: OnceLock<Mutex<Lru<WrapCacheKey, (Vec<(usize, usize)>, bool)>>> =
    OnceLock::new();

static ELLIP_LRU: OnceLock<Mutex<Lru<EllipCacheKey, String>>> = OnceLock::new();

fn wrap_cache() -> &'static Mutex<Lru<WrapCacheKey, (Vec<String>, bool)>> {
    WRAP_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn wrap_ranges_cache() -> &'static Mutex<Lru<WrapCacheKey, (Vec<(usize, usize)>, bool)>> {
    WRAP_RANGES_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn ellip_cache() -> &'static Mutex<Lru<EllipCacheKey, String>> {
    ELLIP_LRU.get_or_init(|| Mutex::new(Lru::new(ELLIP_CACHE_CAP)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey(pub u64);

/// Cache key for the renderer's glyph slug cache -> uniquely identifies a
/// specific glyph in a specific font face.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub font_id: u64,
    pub glyph_id: u32,
    pub font_size_bits: u32,
}

static GLYPH_ID_WARNED: OnceLock<Mutex<std::collections::HashSet<(u64, u32)>>> = OnceLock::new();

/// Map a `u32` glyph id to the `u16` id swash accepts.
///
/// Glyph ids above `u16::MAX` (possible in very large fonts) cannot be
/// rendered by swash. Remap those to `.notdef` (0) and emit a warn once per
/// `(font_id, glyph_id)` instead of silently dropping the glyph. The public
/// [`CacheKey`] keeps the full `u32` id so no API changes are needed.
fn swash_glyph_id(font_id: u64, glyph_id: u32) -> u16 {
    match u16::try_from(glyph_id) {
        Ok(v) => v,
        Err(_) => {
            let warned =
                GLYPH_ID_WARNED.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
            let is_new = warned
                .lock()
                .map(|mut g| g.insert((font_id, glyph_id)))
                .unwrap_or(false);
            if is_new {
                log::warn!(
                    "glyph id {} (font {}) exceeds u16::MAX; falling back to .notdef (0)",
                    glyph_id,
                    font_id
                );
            }
            0
        }
    }
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
    face_index: u32,
}

struct Engine {
    font_cx: parley::FontContext,
    layout_cx: parley::LayoutContext<()>,
    swash_cx: swash::scale::ScaleContext,
    key_map: HashMap<GlyphKey, (u64, u32)>,
    font_registry: Vec<FontRecord>,
    next_font_id: u64,
    /// Cache of rendered glyphs keyed by (font_id, glyph_id, font_size_bits).
    /// Contains (width, height, left, top, content, data).
    glyph_cache:
        HashMap<(u64, u32, u32), (u32, u32, i32, i32, swash::scale::image::Content, Vec<u8>)>,
    /// Cache of (ascent, descent) in px keyed by
    /// (family hash, weight, px bits). Used for baseline alignment.
    ascent_cache: RapidHashMap<(Option<String>, u16, u32, u64), (f32, f32)>,
}

impl Engine {
    fn ensure_font(&mut self, fd: &parley::FontData) -> u64 {
        if let Some(existing) = self
            .font_registry
            .iter()
            .find(|r| r.face_index == fd.index && r.data.data == fd.data)
        {
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
            face_index: fd.index,
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

    /// Resolve `(ascent, descent)` in px for the primary font matching
    /// `(family, weight)`, trying named families first and falling back to
    /// the bundled sans. Returns a `0.8em`/`0.2em` estimate when unresolved.
    fn resolve_vertical_metrics(
        &mut self,
        font_family: Option<&str>,
        font_weight: u16,
        px: f32,
    ) -> (f32, f32) {
        let mut candidates: Vec<&str> = Vec::new();
        match font_family {
            Some("monospace") => candidates.push("JetBrains Mono"),
            Some("sans-serif") => candidates.push("Open Sans"),
            Some("emoji") => candidates.push("Noto Color Emoji"),
            Some(other) => candidates.push(other),
            None => {}
        }
        candidates.push("Open Sans");
        for name in candidates {
            if let Some(m) = self.metrics_for_family(name, font_weight, px) {
                return m;
            }
        }
        (px * 0.8, px * 0.2)
    }

    /// Best-weight-match `(ascent, descent)` for a named family, or `None`
    /// when the family (or its data) is unavailable.
    fn metrics_for_family(&mut self, name: &str, font_weight: u16, px: f32) -> Option<(f32, f32)> {
        let info = self.font_cx.collection.family_by_name(name)?;
        let target = font_weight as f32;
        let mut best: Option<(f32, Vec<u8>, u32)> = None;
        for font in info.fonts() {
            let dist = (font.weight().value() - target).abs();
            if best.as_ref().is_some_and(|(bd, _, _)| *bd <= dist) {
                continue;
            }
            let bytes: Vec<u8> = match font.source().kind() {
                parley::fontique::SourceKind::Memory(blob) => blob.as_ref().to_vec(),
                #[cfg(not(target_arch = "wasm32"))]
                parley::fontique::SourceKind::Path(path) => std::fs::read(path).ok()?,
                #[cfg(target_arch = "wasm32")]
                parley::fontique::SourceKind::Path(_) => continue,
            };
            best = Some((dist, bytes, font.index()));
        }
        let (_, bytes, index) = best?;
        let font = skrifa::FontRef::from_index(&bytes, index).ok()?;
        let metrics = font.metrics(
            skrifa::instance::Size::new(px),
            skrifa::instance::LocationRef::default(),
        );
        Some((metrics.ascent.max(0.0), metrics.descent.abs().max(0.0)))
    }

    fn raster_placement(
        &mut self,
        font_id: u64,
        glyph_id: u32,
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
        let (data_bytes, face_index) = {
            let record = self.font_registry.iter().find(|r| r.id == font_id)?;
            (record.data_bytes.clone(), record.face_index)
        };
        let face_index = usize::try_from(face_index).ok()?;
        let font = swash::FontRef::from_index(&data_bytes, face_index)?;
        let mut scaler = self.swash_cx.builder(font).size(px).hint(true).build();
        let image = Render::new(&[
            Source::Outline,
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
        ])
        .render(&mut scaler, swash_glyph_id(font_id, glyph_id))?;
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

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

pub static FONT_PROVIDER: OnceLock<Mutex<font_awl::Provider>> = OnceLock::new();
#[cfg(target_arch = "wasm32")]
static RETAINED_FONT_DATA: OnceLock<Mutex<Vec<Vec<u8>>>> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
static WASM_FONT_CONTEXT_READY: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(target_arch = "wasm32")]
static WASM_FONT_INIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn append_generic_family_once(
    collection: &mut parley::fontique::Collection,
    generic: parley::fontique::GenericFamily,
    id: parley::fontique::FamilyId,
) {
    let mut families: Vec<parley::fontique::FamilyId> =
        collection.generic_families(generic).collect();
    if !families.contains(&id) {
        families.push(id);
        collection.set_generic_families(generic, families.into_iter());
    }
}

fn configure_collection(collection: &mut parley::fontique::Collection) {
    for name in [
        "Noto Sans Symbols",
        "Noto Sans Symbols 2",
        "Material Symbols Outlined",
    ] {
        if let Some(info) = collection.family_by_name(name) {
            append_generic_family_once(
                collection,
                parley::fontique::GenericFamily::SansSerif,
                info.id(),
            );
        }
    }
    if let Some(info) = collection.family_by_name("Noto Color Emoji") {
        append_generic_family_once(
            collection,
            parley::fontique::GenericFamily::Emoji,
            info.id(),
        );
    }
}

fn register_asset_if_missing(provider: &mut font_awl::Provider, bytes: &[u8]) {
    if collection_font_data_families(provider.collection_mut(), bytes).is_empty() {
        let blob: parley::fontique::Blob<u8> = bytes.to_vec().into();
        let families = provider.collection_mut().register_fonts(blob, None);
        append_registered_families(provider.collection_mut(), &families);
    }
}

fn init_provider_sync() -> font_awl::Provider {
    let mut provider = font_awl::Provider::new();
    provider.load_bundled_fonts();
    static MATERIAL_SYMBOLS_TTF: &[u8] = include_bytes!("assets/MaterialSymbolsOutlined.ttf");
    static NOTO_SYMBOLS_TTF: &[u8] = include_bytes!("assets/NotoSansSymbols2-Regular.ttf");
    static NOTO_EMOJI_TTF: &[u8] = include_bytes!("assets/NotoColorEmoji-Regular.ttf");
    register_asset_if_missing(&mut provider, MATERIAL_SYMBOLS_TTF);
    register_asset_if_missing(&mut provider, NOTO_SYMBOLS_TTF);
    register_asset_if_missing(&mut provider, NOTO_EMOJI_TTF);
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(e) = provider.load_system_fonts_best_effort() {
        log::warn!("font-awl: failed to load system fonts: {e}");
    }
    configure_collection(provider.collection_mut());
    provider
}

fn provider() -> &'static Mutex<font_awl::Provider> {
    FONT_PROVIDER.get_or_init(|| Mutex::new(init_provider_sync()))
}

fn init_engine_sync() -> Engine {
    let mut font_cx = provider().lock().unwrap().new_parley_context();
    configure_collection(&mut font_cx.collection);

    Engine {
        font_cx,
        layout_cx: parley::LayoutContext::new(),
        swash_cx: swash::scale::ScaleContext::new(),
        key_map: HashMap::new(),
        font_registry: Vec::new(),
        next_font_id: 1,
        glyph_cache: HashMap::new(),
        ascent_cache: RapidHashMap::new(),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn init_fonts_wasm() {
    if WASM_FONT_CONTEXT_READY.load(Ordering::Acquire) {
        return;
    }
    if WASM_FONT_INIT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let mut candidate = init_provider_sync();
    let result = candidate.load_web_fonts().await;
    if let Err(error) = result {
        WASM_FONT_INIT.store(false, Ordering::Release);
        log::warn!("font-awl: failed to load web fonts: {error}");
        return;
    }
    restore_retained_font_data(candidate.collection_mut());
    configure_collection(candidate.collection_mut());
    {
        let mut p = provider().lock().unwrap();
        *p = candidate;
    }
    let font_cx = provider().lock().unwrap().new_parley_context();
    if let Some(eng) = ENGINE.get() {
        let mut eng = eng.lock().unwrap();
        eng.font_cx = font_cx;
        clear_caches_for_fallback_in(&mut eng);
    } else {
        drop(font_cx);
    }
    WASM_FONT_CONTEXT_READY.store(true, Ordering::Release);
    WASM_FONT_INIT.store(false, Ordering::Release);
}

fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| Mutex::new(init_engine_sync()))
}

pub fn register_font_data(bytes: &[u8]) {
    let _ = register_font_data_if_usable(bytes);
}

fn font_source_bytes(font: &parley::fontique::FontInfo) -> Option<&[u8]> {
    match font.source().kind() {
        parley::fontique::SourceKind::Memory(blob) => Some(blob.as_ref()),
        parley::fontique::SourceKind::Path(_) => None,
    }
}

fn collection_font_data_families(
    collection: &mut parley::fontique::Collection,
    bytes: &[u8],
) -> Vec<parley::fontique::FamilyId> {
    let mut names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    names.sort_unstable();
    let mut result = Vec::new();
    for name in names {
        let Some(info) = collection.family_by_name(&name) else {
            continue;
        };
        if info
            .fonts()
            .iter()
            .any(|font| font_source_bytes(font).is_some_and(|source| source == bytes))
        {
            result.push(info.id());
        }
    }
    result
}

fn append_registered_families(
    collection: &mut parley::fontique::Collection,
    families: &[(parley::fontique::FamilyId, Vec<parley::fontique::FontInfo>)],
) {
    let mut ids: Vec<parley::fontique::FamilyId> = families.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    for id in ids {
        let Some(name) = collection.family_name(id).map(str::to_owned) else {
            continue;
        };
        let generic = if name.starts_with("Noto Color Emoji") {
            parley::fontique::GenericFamily::Emoji
        } else {
            parley::fontique::GenericFamily::SansSerif
        };
        append_generic_family_once(collection, generic, id);
    }
}

#[cfg(target_arch = "wasm32")]
fn retain_font_data(bytes: &[u8]) {
    let retained = RETAINED_FONT_DATA.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut retained) = retained.lock() else {
        return;
    };
    if retained.iter().all(|existing| existing.as_slice() != bytes) {
        retained.push(bytes.to_vec());
    }
}

#[cfg(target_arch = "wasm32")]
fn restore_retained_font_data(collection: &mut parley::fontique::Collection) {
    let retained = RETAINED_FONT_DATA
        .get()
        .and_then(|retained| retained.lock().ok().map(|retained| retained.clone()));
    let Some(retained) = retained else {
        return;
    };
    for bytes in retained {
        if collection_font_data_families(collection, &bytes).is_empty() {
            let blob: parley::fontique::Blob<u8> = bytes.into();
            let families = collection.register_fonts(blob, None);
            append_registered_families(collection, &families);
        }
    }
}

pub(crate) fn register_font_data_if_usable(bytes: &[u8]) -> bool {
    let (font_cx, newly_registered) = {
        let mut p = provider().lock().unwrap();
        let existing = collection_font_data_families(p.collection_mut(), bytes);
        if !existing.is_empty() {
            for id in existing {
                let is_emoji = p
                    .collection_mut()
                    .family_name(id)
                    .is_some_and(|name| name.starts_with("Noto Color Emoji"));
                let generic = if is_emoji {
                    parley::fontique::GenericFamily::Emoji
                } else {
                    parley::fontique::GenericFamily::SansSerif
                };
                append_generic_family_once(p.collection_mut(), generic, id);
            }
            (p.new_parley_context(), false)
        } else {
            let blob: parley::fontique::Blob<u8> = bytes.to_vec().into();
            let families = p.collection_mut().register_fonts(blob, None);
            if families.is_empty() {
                return false;
            }
            append_registered_families(p.collection_mut(), &families);
            configure_collection(p.collection_mut());
            (p.new_parley_context(), true)
        }
    };

    if newly_registered {
        #[cfg(target_arch = "wasm32")]
        retain_font_data(bytes);
        let mut eng = engine().lock().unwrap();
        eng.font_cx = font_cx;
        clear_caches_for_fallback_in(&mut eng);
    }

    true
}

pub(crate) fn clear_caches_for_fallback_in(eng: &mut Engine) {
    clear_lru_caches();
    eng.ascent_cache.clear();
    bump_frame_for_fallback();
}

fn clear_lru_caches() {
    if let Some(c) = METRICS_LRU.get()
        && let Ok(mut g) = c.lock()
    {
        g.clear_both();
    }
    if let Some(c) = WRAP_LRU.get()
        && let Ok(mut g) = c.lock()
    {
        g.clear_both();
    }
    if let Some(c) = WRAP_RANGES_LRU.get()
        && let Ok(mut g) = c.lock()
    {
        g.clear_both();
    }
    if let Some(c) = ELLIP_LRU.get()
        && let Ok(mut g) = c.lock()
    {
        g.clear_both();
    }
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

/// Vertical font metrics `(ascent, descent)` in px for the primary font
/// matching `(font_family, font_weight)`.
///
/// Used for text baseline alignment (`AlignItems::Baseline`): the first
/// baseline of a text block sits `ascent` below the top of its first line's
/// em box (plus half-leading, applied by the caller). Results are cached.
/// Falls back to a `0.8em`/`0.2em` estimate when no font resolves.
pub fn primary_font_vertical_metrics(
    font_family: Option<&str>,
    font_weight: u16,
    px: f32,
) -> (f32, f32) {
    if !(px > 0.0) {
        return (0.0, 0.0);
    }
    let key = (
        font_family.map(str::to_owned),
        font_weight,
        px.to_bits(),
        font_generation(),
    );
    let mut eng = engine().lock().unwrap();
    if let Some(&m) = eng.ascent_cache.get(&key) {
        return m;
    }
    let m = eng.resolve_vertical_metrics(font_family, font_weight, px);
    eng.ascent_cache.insert(key, m);
    m
}

/// Extract the family name from raw font bytes.
///
/// Tries the typographic family name first, falling back to the standard family name.
/// Returns `None` if the font data is invalid or contains no names.
pub fn font_family_name(bytes: &[u8]) -> Option<String> {
    use skrifa::string::StringId;

    for face_index in 0..4096u32 {
        let Ok(font) = skrifa::FontRef::from_index(bytes, face_index) else {
            break;
        };
        let name = font
            .localized_strings(StringId::TYPOGRAPHIC_FAMILY_NAME)
            .english_or_first()
            .map(|s| s.to_string())
            .or_else(|| {
                font.localized_strings(StringId::FAMILY_NAME)
                    .english_or_first()
                    .map(|s| s.to_string())
            });
        if name.is_some() {
            return name;
        }
    }
    None
}

fn key_from_pair(font_id: u64, glyph_id: u32) -> GlyphKey {
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
                    if let Some(slice) = text.get(start..end) {
                        for ch in slice.chars() {
                            out.push(ch as u32);
                        }
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

fn push_font_family<'a>(
    builder: &mut parley::RangedBuilder<'a, ()>,
    family: Option<&str>,
    text_len: usize,
) {
    use parley::style::{FontFamilyName, GenericFamily};

    let mut names = Vec::with_capacity(10);
    match family {
        Some("monospace") => {
            names.push(FontFamilyName::named("JetBrains Mono"));
            names.push(GenericFamily::Monospace.into());
        }
        Some("sans-serif") => names.push(FontFamilyName::named("Open Sans")),
        Some("emoji") => names.push(FontFamilyName::named("Noto Color Emoji")),
        Some("serif") => names.push(GenericFamily::Serif.into()),
        Some("cursive") => names.push(GenericFamily::Cursive.into()),
        Some("fantasy") => names.push(GenericFamily::Fantasy.into()),
        Some("system-ui") => names.push(GenericFamily::SystemUi.into()),
        Some("math") => names.push(GenericFamily::Math.into()),
        Some(family) => names.push(FontFamilyName::named(family)),
        None => {}
    }
    names.push(GenericFamily::SansSerif.into());
    names.push(GenericFamily::Emoji.into());
    names.push(FontFamilyName::named("Noto Color Emoji"));
    names.push(FontFamilyName::named("Noto Sans Symbols 2"));
    names.push(FontFamilyName::named("Noto Sans Symbols2"));
    names.push(FontFamilyName::named("Noto Sans Symbols"));
    names.push(FontFamilyName::named("Material Symbols Outlined"));
    builder.push(names.as_slice(), 0..text_len);
}

fn build_layout(
    eng: &mut Engine,
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> parley::Layout<()> {
    use parley::FontWeight;
    use parley::style::StyleProperty;

    let mut builder = eng
        .layout_cx
        .ranged_builder(&mut eng.font_cx, text, 1.0, true);
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
    push_font_family(&mut builder, font_family, text.len());
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    layout.align(
        parley::Alignment::Start,
        parley::AlignmentOptions::default(),
    );
    layout
}

#[cfg(target_arch = "wasm32")]
fn report_unresolved_codepoints(layout: &parley::Layout<()>, text: &str) {
    let unresolved = collect_unresolved_codepoints(layout, text);
    if unresolved.is_empty() {
        return;
    }
    let reg = crate::unresolved::web_unresolved_registry();
    if unresolved.iter().any(|cp| !reg.contains(*cp)) {
        crate::fallback::wasm_fallback::ensure_fallback_initialized();
        reg.add_unresolved_vec(unresolved);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn report_unresolved_codepoints(_layout: &parley::Layout<()>, _text: &str) {}

fn collect_shaped_layout(
    eng: &mut Engine,
    layout: parley::Layout<()>,
    px: f32,
    collect_runs: bool,
) -> (Vec<ShapedGlyph>, Vec<ShapedRun>) {
    use parley::layout::PositionedLayoutItem;

    let mut glyphs = Vec::new();
    let mut runs = Vec::new();
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            if collect_runs {
                runs.push(ShapedRun {
                    text_range: run.text_range(),
                    rtl: run.is_rtl(),
                    synthesis: run.synthesis(),
                });
            }
            let font_data = run.font();
            let fid = eng.ensure_font(font_data);
            log::debug!(
                "[shape] run: fid={} font_data_len={}",
                fid,
                font_data.data.as_ref().len()
            );
            for glyph in glyph_run.positioned_glyphs() {
                let gid = glyph.id;
                let key = key_from_pair(fid, gid);
                eng.key_map.insert(key, (fid, gid));
                let (width, height, left, top) = eng
                    .raster_placement(fid, gid, px)
                    .unwrap_or((0.0, 0.0, 0.0, 0.0));
                log::debug!(
                    "[shape] glyph: gid={} px={} x={:.1} y={:.1} advance={:.1} bitmap={}x{} {}x{}",
                    gid,
                    px,
                    glyph.x,
                    glyph.y,
                    glyph.advance,
                    width,
                    height,
                    left,
                    top,
                );
                glyphs.push(ShapedGlyph {
                    key,
                    px,
                    x: glyph.x,
                    y: glyph.y,
                    w: width,
                    h: height,
                    bearing_x: left,
                    bearing_y: top,
                    advance: glyph.advance,
                });
            }
        }
    }
    (glyphs, runs)
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
    let layout = build_layout(
        eng,
        text,
        px,
        line_height_ratio,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    report_unresolved_codepoints(&layout, text);
    let (out, _) = collect_shaped_layout(eng, layout, px, false);
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontSynthesis {
    #[default]
    Unspecified,
    None,
    Weight,
    Style,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeCapabilityError {
    RequestedDirectionMismatch,
    SynthesisModeUnsupported,
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeOptions<'a> {
    pub font_family: Option<&'a str>,
    pub font_weight: u16,
    pub font_style: u8,
    pub letter_spacing: f32,
    pub font_variation_settings: Option<&'a str>,
    pub text_direction: TextDirection,
    pub font_synthesis: FontSynthesis,
}

impl Default for ShapeOptions<'_> {
    fn default() -> Self {
        Self {
            font_family: None,
            font_weight: 400,
            font_style: 0,
            letter_spacing: 0.0,
            font_variation_settings: None,
            text_direction: TextDirection::Auto,
            font_synthesis: FontSynthesis::Unspecified,
        }
    }
}

pub struct ShapedRun {
    pub text_range: std::ops::Range<usize>,
    pub rtl: bool,
    pub synthesis: parley::fontique::Synthesis,
}

pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
    pub requested_text_direction: TextDirection,
    pub resolved_text_direction: TextDirection,
    pub runs: Vec<ShapedRun>,
}

fn synthesis_has_weight(synthesis: &parley::fontique::Synthesis) -> bool {
    synthesis.embolden()
        || synthesis
            .variation_settings()
            .iter()
            .any(|(tag, _)| tag.to_be_bytes() == *b"wght")
}

fn synthesis_has_style(synthesis: &parley::fontique::Synthesis) -> bool {
    synthesis.skew().is_some()
        || synthesis.variation_settings().iter().any(|(tag, _)| {
            matches!(
                tag.to_be_bytes(),
                [b'i', b't', b'a', b'l'] | [b's', b'l', b'n', b't']
            )
        })
}

fn validate_shape_options(
    options: ShapeOptions<'_>,
    resolved_direction: TextDirection,
    runs: &[ShapedRun],
) -> Result<(), ShapeCapabilityError> {
    if options.text_direction != TextDirection::Auto && options.text_direction != resolved_direction
    {
        return Err(ShapeCapabilityError::RequestedDirectionMismatch);
    }
    let invalid = match options.font_synthesis {
        FontSynthesis::Unspecified | FontSynthesis::All => false,
        FontSynthesis::None => runs.iter().any(|run| run.synthesis.any()),
        FontSynthesis::Weight => runs.iter().any(|run| synthesis_has_style(&run.synthesis)),
        FontSynthesis::Style => runs.iter().any(|run| synthesis_has_weight(&run.synthesis)),
    };
    if invalid {
        Err(ShapeCapabilityError::SynthesisModeUnsupported)
    } else {
        Ok(())
    }
}

pub fn shape_text_with_options(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<ShapedText, ShapeCapabilityError> {
    let mut eng = engine().lock().unwrap();
    let layout = build_layout(
        &mut eng,
        text,
        px,
        line_height_ratio,
        options.font_family,
        options.font_weight,
        options.font_style,
        options.letter_spacing,
        options.font_variation_settings,
    );
    report_unresolved_codepoints(&layout, text);
    let resolved_direction = if layout.is_rtl() {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    };
    let (glyphs, runs) = collect_shaped_layout(&mut eng, layout, px, true);
    validate_shape_options(options, resolved_direction, &runs)?;
    Ok(ShapedText {
        glyphs,
        requested_text_direction: options.text_direction,
        resolved_text_direction: resolved_direction,
        runs,
    })
}

pub fn shape_line_with_options(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Vec<ShapedGlyph>, ShapeCapabilityError> {
    shape_text_with_options(text, px, line_height_ratio, options).map(|shaped| shaped.glyphs)
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
    let (data_bytes, face_index) = {
        let record = eng.font_registry.iter().find(|r| r.id == fid)?;
        (record.data_bytes.clone(), record.face_index)
    };
    let face_index = usize::try_from(face_index).ok()?;
    let font = swash::FontRef::from_index(&data_bytes, face_index)?;
    let mut scaler = eng.swash_cx.builder(font).size(px).hint(true).build();
    let image = Render::new(&[
        Source::Outline,
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::ColorOutline(0),
    ])
    .render(&mut scaler, swash_glyph_id(fid, gid))?;
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

fn extract_outlines_for(
    data_bytes: &[u8],
    face_index: u32,
    glyph_id: u32,
) -> Option<Box<[Command]>> {
    let font = skrifa::FontRef::from_index(data_bytes, face_index).ok()?;
    let mut pen = OutlinePenCollector(Vec::new());
    font.outline_glyphs()
        .get(skrifa::GlyphId::new(glyph_id))?
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
    extract_outlines_for(&record.data_bytes, record.face_index, cache_key.glyph_id)
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
    let cmds = extract_outlines_for(&record.data_bytes, record.face_index, gid)?;
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

#[derive(Clone, Copy)]
struct LogicalPart {
    start: usize,
    end: usize,
    advance: f32,
}

struct LogicalGroup {
    start: usize,
    end: usize,
    parts: Vec<LogicalPart>,
    has_ligature_start: bool,
    advance: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct AtomicRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy)]
struct VisualExtent {
    start: usize,
    end: usize,
    left: f32,
    right: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeKind {
    Start,
    Internal,
    End,
}

#[derive(Clone, Copy)]
struct EdgeCandidate {
    byte: usize,
    position: f32,
    kind: EdgeKind,
}

struct LayoutEdges {
    edges: Vec<(usize, f32)>,
    extents: Vec<VisualExtent>,
    atomic_ranges: Vec<AtomicRange>,
}

struct LineMeasurement {
    metrics: TextMetrics,
    extents: Vec<VisualExtent>,
    atomic_ranges: Vec<AtomicRange>,
}

#[derive(Clone, Copy)]
struct HardLine {
    start: usize,
    end: usize,
    next_start: usize,
}

fn hard_lines(text: &str) -> Vec<HardLine> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        let next_start = match ch {
            '\n' => Some(index + 1),
            '\u{2028}' | '\u{2029}' => Some(index + ch.len_utf8()),
            '\r' => {
                if text[index + 1..].starts_with('\n') {
                    chars.next();
                    Some(index + 2)
                } else {
                    Some(index + 1)
                }
            }
            _ => None,
        };
        if let Some(next_start) = next_start {
            lines.push(HardLine {
                start,
                end: index,
                next_start,
            });
            start = next_start;
        }
    }
    lines.push(HardLine {
        start,
        end: text.len(),
        next_start: text.len(),
    });
    lines
}

fn finish_logical_group(mut group: LogicalGroup) -> LogicalGroup {
    for part in &mut group.parts {
        if !part.advance.is_finite() {
            part.advance = 0.0;
        }
    }
    group.advance = group.parts.iter().map(|part| part.advance).sum();
    if !group.advance.is_finite() {
        group.advance = 0.0;
    }
    group.end = group.end.max(group.start);
    group
}

fn logical_groups_for_run<B: parley::style::Brush>(
    run: parley::layout::Run<'_, B>,
) -> Vec<LogicalGroup> {
    let mut groups = Vec::new();
    let mut current: Option<LogicalGroup> = None;
    for cluster in run.clusters() {
        let range = cluster.text_range();
        let part = LogicalPart {
            start: range.start,
            end: range.end,
            advance: cluster.advance(),
        };
        let is_ligature = cluster.is_ligature_start() || cluster.is_ligature_continuation();
        if !is_ligature {
            if let Some(group) = current.take() {
                groups.push(finish_logical_group(group));
            }
            groups.push(finish_logical_group(LogicalGroup {
                start: part.start,
                end: part.end,
                parts: vec![part],
                has_ligature_start: false,
                advance: 0.0,
            }));
            continue;
        }
        let starts_new_group = cluster.is_ligature_start()
            && current
                .as_ref()
                .is_some_and(|group| group.has_ligature_start);
        if starts_new_group && let Some(group) = current.take() {
            groups.push(finish_logical_group(group));
        }
        let group = current.get_or_insert_with(|| LogicalGroup {
            start: part.start,
            end: part.end,
            parts: Vec::new(),
            has_ligature_start: false,
            advance: 0.0,
        });
        group.start = group.start.min(part.start);
        group.end = group.end.max(part.end);
        group.parts.push(part);
        group.has_ligature_start |= cluster.is_ligature_start();
    }
    if let Some(group) = current {
        groups.push(finish_logical_group(group));
    }
    groups
}

fn add_grapheme_edges(
    text: &str,
    range: std::ops::Range<usize>,
    start_x: f32,
    end_x: f32,
    candidates: &mut Vec<EdgeCandidate>,
) {
    let start = range.start.min(text.len());
    let end = range.end.min(text.len()).max(start);
    let Some(slice) = text.get(start..end) else {
        return;
    };
    let mut boundaries = vec![start];
    boundaries.extend(
        slice
            .grapheme_indices(true)
            .map(|(offset, grapheme)| start + offset + grapheme.len()),
    );
    if boundaries.len() == 1 {
        candidates.push(EdgeCandidate {
            byte: start,
            position: start_x,
            kind: EdgeKind::Start,
        });
        candidates.push(EdgeCandidate {
            byte: end,
            position: end_x,
            kind: EdgeKind::End,
        });
        return;
    }
    let count = boundaries.len() - 1;
    for (index, byte) in boundaries.into_iter().enumerate() {
        let fraction = index as f32 / count as f32;
        let position = start_x + (end_x - start_x) * fraction;
        let kind = if index == 0 {
            EdgeKind::Start
        } else if index == count {
            EdgeKind::End
        } else {
            EdgeKind::Internal
        };
        candidates.push(EdgeCandidate {
            byte,
            position,
            kind,
        });
    }
}

fn edge_priority(kind: EdgeKind) -> u8 {
    match kind {
        EdgeKind::Internal => 0,
        EdgeKind::Start => 1,
        EdgeKind::End => 2,
    }
}

fn resolve_edge_candidates(
    mut candidates: Vec<EdgeCandidate>,
    text_len: usize,
) -> Vec<(usize, f32)> {
    candidates.sort_by_key(|candidate| candidate.byte);
    let mut edges = BTreeMap::new();
    for candidate in candidates {
        let replace = match edges.get(&candidate.byte) {
            None => true,
            Some((_, kind)) => edge_priority(candidate.kind) > edge_priority(*kind),
        };
        if replace {
            edges.insert(candidate.byte, (candidate.position, candidate.kind));
        }
    }
    edges.entry(0).or_insert((0.0, EdgeKind::Start));
    edges.entry(text_len).or_insert((0.0, EdgeKind::End));
    edges
        .into_iter()
        .map(|(byte, (position, _))| (byte, position))
        .collect()
}

fn normalized_atomic_ranges(mut ranges: Vec<AtomicRange>) -> Vec<AtomicRange> {
    ranges.retain(|range| range.end > range.start);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut result: Vec<AtomicRange> = Vec::new();
    for range in ranges {
        if let Some(previous) = result.last_mut()
            && (range.start < previous.end || range.start == previous.start)
        {
            previous.end = previous.end.max(range.end);
        } else {
            result.push(range);
        }
    }
    result
}

fn atomic_ranges_for_layout(text: &str, layout: &parley::Layout<()>) -> Vec<AtomicRange> {
    let mut clusters = Vec::new();
    let mut seen = HashSet::new();
    for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                let range = cluster.text_range();
                let candidate = (
                    range.start.min(text.len()),
                    range.end.min(text.len()),
                    cluster.is_ligature_start(),
                    cluster.is_ligature_continuation(),
                );
                if seen.insert(candidate) {
                    clusters.push(candidate);
                }
            }
        }
    }
    clusters.sort_by_key(|(start, end, _, _)| (*start, *end));
    let mut result = Vec::new();
    let mut current: Option<AtomicRange> = None;
    let mut has_start = false;
    for (start, end, is_start, is_continuation) in clusters {
        if end <= start {
            continue;
        }
        if !is_start && !is_continuation {
            if let Some(range) = current.take() {
                result.push(range);
            }
            has_start = false;
            result.push(AtomicRange { start, end });
            continue;
        }
        if is_start
            && has_start
            && let Some(range) = current.take()
        {
            result.push(range);
            has_start = false;
        }
        let range = current.get_or_insert(AtomicRange { start, end });
        range.start = range.start.min(start);
        range.end = range.end.max(end);
        has_start |= is_start;
    }
    if let Some(range) = current {
        result.push(range);
    }
    normalized_atomic_ranges(result)
}

fn layout_edges_for_layout(text: &str, layout: &parley::Layout<()>) -> LayoutEdges {
    let mut candidates = Vec::new();
    let mut extents = Vec::new();

    for line in layout.lines() {
        let line_range = line.text_range();
        let line_start = line_range.start.min(text.len());
        let line_end = line_range.end.min(text.len()).max(line_start);
        candidates.push(EdgeCandidate {
            byte: line_start,
            position: 0.0,
            kind: EdgeKind::Internal,
        });
        if line_end > line_start {
            candidates.push(EdgeCandidate {
                byte: line_end,
                position: 0.0,
                kind: EdgeKind::Internal,
            });
        }

        let inline_min = line.metrics().inline_min_coord;
        for run in line.runs() {
            let mut visual_clusters = HashMap::new();
            for cluster in run.visual_clusters() {
                let range = cluster.text_range();
                visual_clusters
                    .entry((range.start, range.end))
                    .or_insert_with(|| cluster.visual_offset());
            }
            let groups = logical_groups_for_run(run);
            for group in &groups {
                let mut left = f32::INFINITY;
                let mut right = f32::NEG_INFINITY;
                for part in &group.parts {
                    let Some(offset) = visual_clusters
                        .get(&(part.start, part.end))
                        .copied()
                        .flatten()
                    else {
                        continue;
                    };
                    let offset = offset + inline_min;
                    let advance = if part.advance.is_finite() {
                        part.advance
                    } else {
                        0.0
                    };
                    left = left.min(offset).min(offset + advance);
                    right = right.max(offset).max(offset + advance);
                    let (start_x, end_x) = if run.is_rtl() {
                        (offset + advance, offset)
                    } else {
                        (offset, offset + advance)
                    };
                    add_grapheme_edges(text, part.start..part.end, start_x, end_x, &mut candidates);
                }
                if left.is_finite() && right.is_finite() {
                    extents.push(VisualExtent {
                        start: group.start.min(text.len()),
                        end: group.end.min(text.len()),
                        left,
                        right,
                    });
                }
            }
        }
    }

    LayoutEdges {
        edges: resolve_edge_candidates(candidates, text.len()),
        extents,
        atomic_ranges: atomic_ranges_for_layout(text, layout),
    }
}

fn visual_width(measurement: &LineMeasurement, start: usize, end: usize) -> f32 {
    let text_len = measurement
        .metrics
        .byte_offsets
        .last()
        .copied()
        .unwrap_or(0);
    let start = start.min(text_len);
    let end = end.min(text_len).max(start);
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for extent in &measurement.extents {
        if extent.start < end && extent.end > start {
            min = min.min(extent.left).min(extent.right);
            max = max.max(extent.left).max(extent.right);
        }
    }
    for (byte, position) in measurement
        .metrics
        .byte_offsets
        .iter()
        .zip(&measurement.metrics.positions)
    {
        if *byte >= start && *byte <= end {
            min = min.min(*position);
            max = max.max(*position);
        }
    }
    if min.is_finite() && max.is_finite() {
        (max - min).max(0.0)
    } else {
        0.0
    }
}

fn edge_position(edges: &[(usize, f32)], byte: usize) -> f32 {
    match edges.binary_search_by_key(&byte, |edge| edge.0) {
        Ok(index) => edges[index].1,
        Err(0) => edges.first().map(|edge| edge.1).unwrap_or(0.0),
        Err(index) => edges
            .get(index.saturating_sub(1))
            .map(|edge| edge.1)
            .unwrap_or(0.0),
    }
}

fn metrics_from_edges(text: &str, edges: &[(usize, f32)]) -> TextMetrics {
    let mut byte_offsets = Vec::with_capacity(text.graphemes(true).count() + 1);
    byte_offsets.push(0);
    for (byte, _) in text.grapheme_indices(true) {
        if byte != 0 {
            byte_offsets.push(byte);
        }
    }
    if *byte_offsets.last().unwrap_or(&0) != text.len() {
        byte_offsets.push(text.len());
    }
    let positions = byte_offsets
        .iter()
        .map(|byte| edge_position(edges, *byte))
        .collect();
    TextMetrics {
        positions,
        byte_offsets,
    }
}

fn measure_line_with_engine(
    eng: &mut Engine,
    text: &str,
    px: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> LineMeasurement {
    let layout = build_layout(
        eng,
        text,
        px,
        0.0,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    report_unresolved_codepoints(&layout, text);
    let layout_edges = layout_edges_for_layout(text, &layout);
    let metrics = metrics_from_edges(text, &layout_edges.edges);
    LineMeasurement {
        metrics,
        extents: layout_edges.extents,
        atomic_ranges: layout_edges.atomic_ranges,
    }
}

fn measure_text_with_engine(
    eng: &mut Engine,
    text: &str,
    px: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> TextMetrics {
    let mut edges = BTreeMap::new();
    for line in hard_lines(text) {
        let local_text = &text[line.start..line.end];
        let layout = build_layout(
            eng,
            local_text,
            px,
            0.0,
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
        );
        report_unresolved_codepoints(&layout, local_text);
        let local_edges = layout_edges_for_layout(local_text, &layout);
        for (byte, position) in local_edges.edges {
            edges.insert(line.start + byte, position);
        }
    }
    for line in hard_lines(text) {
        if line.next_start > line.end {
            edges.insert(line.next_start, 0.0);
        }
    }
    let edges = edges.into_iter().collect::<Vec<_>>();
    metrics_from_edges(text, &edges)
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
    let key = MetricsCacheKey {
        text: text.to_owned(),
        px_bits: px.to_bits(),
        family: font_family.map(str::to_owned),
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(str::to_owned),
        generation: font_generation(),
    };
    if let Some(m) = metrics_cache().lock().unwrap().get(&key).cloned() {
        return m;
    }
    let mut eng = engine().lock().unwrap();
    let metrics = measure_text_with_engine(
        &mut eng,
        text,
        px,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    metrics_cache().lock().unwrap().put(key, metrics.clone());
    metrics
}

fn is_wrapping_space(ch: char) -> bool {
    ch.is_whitespace() && !matches!(ch, '\u{00A0}' | '\u{202F}' | '\u{2007}')
}

fn trim_start_byte(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte < text.len() {
        let ch = text[byte..].chars().next().unwrap();
        if !is_wrapping_space(ch) {
            break;
        }
        byte += ch.len_utf8();
    }
    byte
}

fn trim_end_byte(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while byte > 0 {
        let previous = text[..byte].char_indices().next_back();
        let Some((start, ch)) = previous else {
            break;
        };
        if !is_wrapping_space(ch) {
            break;
        }
        byte = start;
    }
    byte
}

fn containing_atomic_range(ranges: &[AtomicRange], byte: usize) -> Option<AtomicRange> {
    ranges
        .iter()
        .copied()
        .find(|range| range.start < byte && byte < range.end)
}

fn snap_start_to_atomic(ranges: &[AtomicRange], byte: usize) -> usize {
    containing_atomic_range(ranges, byte)
        .map(|range| range.start)
        .unwrap_or(byte)
}

fn snap_end_to_atomic(ranges: &[AtomicRange], byte: usize) -> usize {
    containing_atomic_range(ranges, byte)
        .map(|range| range.end)
        .unwrap_or(byte)
}

fn next_safe_boundary(text: &str, ranges: &[AtomicRange], start: usize, limit: usize) -> usize {
    let start = start.min(text.len());
    let limit = limit.min(text.len()).max(start);
    if start >= limit {
        return start;
    }
    let mut next = next_grapheme_end(text, start, limit);
    while let Some(range) = containing_atomic_range(ranges, next) {
        if range.end >= limit {
            return limit;
        }
        next = range.end;
    }
    next
}

fn push_trimmed_range_atomic(
    text: &str,
    start: usize,
    end: usize,
    ranges: &[AtomicRange],
    out: &mut Vec<(usize, usize)>,
) {
    let mut start = start.min(text.len());
    let mut end = end.min(text.len()).max(start);
    let whitespace_only = text.get(start..end).is_some_and(|slice| {
        slice
            .chars()
            .all(|ch| is_wrapping_space(ch) || matches!(ch, '\u{00A0}' | '\u{202F}' | '\u{2007}'))
    });
    if end == start {
        out.push((start, end));
        return;
    }
    if whitespace_only {
        out.push((start, end));
        return;
    }
    start = trim_start_byte(text, start);
    end = trim_end_byte(text, end.max(start));
    start = snap_start_to_atomic(ranges, start);
    end = snap_end_to_atomic(ranges, end).max(start);
    out.push((start, end));
}

fn next_grapheme_end(text: &str, start: usize, limit: usize) -> usize {
    let start = start.min(text.len());
    let limit = limit.min(text.len());
    if start >= limit {
        return start;
    }
    text.get(start..limit)
        .and_then(|slice| {
            slice
                .grapheme_indices(true)
                .next()
                .map(|(offset, grapheme)| start + offset + grapheme.len())
        })
        .unwrap_or(limit)
}

fn wrap_one_hard_line_ranges(
    text: &str,
    max_width: f32,
    max_lines: Option<usize>,
    measurement: &LineMeasurement,
) -> (Vec<(usize, usize)>, bool) {
    if text.is_empty() {
        return (vec![(0, 0)], false);
    }
    if max_lines == Some(0) {
        return (Vec::new(), true);
    }
    let atomic_ranges = &measurement.atomic_ranges;
    if visual_width(measurement, 0, text.len()) <= max_width + 0.5 {
        let mut out = Vec::new();
        push_trimmed_range_atomic(text, 0, text.len(), atomic_ranges, &mut out);
        return (out, false);
    }

    let mut out = Vec::new();
    let mut truncated = false;
    let mut line_start = trim_start_byte(text, 0);
    line_start = snap_start_to_atomic(atomic_ranges, line_start);
    let mut last_fit = Some(line_start);

    for (raw_token_start, token) in text.split_word_bound_indices() {
        let raw_token_start = raw_token_start.min(text.len());
        let raw_token_end = (raw_token_start + token.len()).min(text.len());
        let token_start = snap_start_to_atomic(atomic_ranges, raw_token_start);
        let token_end = snap_end_to_atomic(atomic_ranges, raw_token_end).max(token_start);
        if token_end <= line_start {
            continue;
        }

        let mut content_end = trim_end_byte(text, token_end);
        content_end = snap_end_to_atomic(atomic_ranges, content_end);
        if content_end <= line_start {
            last_fit = Some(token_end);
            continue;
        }

        if visual_width(measurement, line_start, content_end) <= max_width + 0.5 {
            last_fit = Some(token_end);
            continue;
        }

        if let Some(fit) = last_fit.filter(|fit| *fit > line_start) {
            push_trimmed_range_atomic(text, line_start, fit, atomic_ranges, &mut out);
            if max_lines.is_some_and(|limit| out.len() >= limit) {
                truncated = fit < text.len();
                return (out, truncated);
            }
            line_start = trim_start_byte(text, fit);
            line_start = snap_start_to_atomic(atomic_ranges, line_start);
            last_fit = None;
            if token_end <= line_start {
                continue;
            }
        }

        let mut remaining_start = line_start.max(token_start);
        while remaining_start < token_end {
            remaining_start = trim_start_byte(text, remaining_start);
            remaining_start = snap_start_to_atomic(atomic_ranges, remaining_start);
            if remaining_start >= token_end {
                break;
            }
            let mut cut = line_start;
            let mut probe = remaining_start;
            while probe < token_end {
                let next = next_safe_boundary(text, atomic_ranges, probe, token_end);
                if next <= probe {
                    break;
                }
                if visual_width(measurement, line_start, next) <= max_width + 0.5 {
                    cut = next;
                    probe = next;
                } else {
                    break;
                }
            }
            if cut <= line_start {
                cut = next_safe_boundary(text, atomic_ranges, remaining_start, token_end);
            }
            if cut <= line_start {
                break;
            }
            push_trimmed_range_atomic(text, line_start, cut, atomic_ranges, &mut out);
            line_start = cut;
            if max_lines.is_some_and(|limit| out.len() >= limit) {
                truncated = line_start < text.len();
                return (out, truncated);
            }
            line_start = trim_start_byte(text, line_start);
            line_start = snap_start_to_atomic(atomic_ranges, line_start);
            last_fit = None;
            remaining_start = line_start;
        }
    }

    if line_start < text.len() && max_lines.is_none_or(|limit| out.len() < limit) {
        push_trimmed_range_atomic(text, line_start, text.len(), atomic_ranges, &mut out);
    }
    if out.is_empty() {
        out.push((text.len(), text.len()));
    }
    (out, truncated)
}

fn wrap_cache_key(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> WrapCacheKey {
    WrapCacheKey {
        text: text.to_owned(),
        px_bits: px.to_bits(),
        max_width_bits: max_width.to_bits(),
        max_lines,
        family: font_family.map(str::to_owned),
        soft_wrap,
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(str::to_owned),
        generation: font_generation(),
    }
}

pub fn wrap_lines_with_family(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<String>, bool) {
    if text.is_empty() {
        return if max_lines == Some(0) {
            (Vec::new(), true)
        } else {
            (vec![String::new()], false)
        };
    }
    if max_width <= 0.0 && soft_wrap {
        return if max_lines == Some(0) {
            (Vec::new(), true)
        } else {
            (vec![String::new()], false)
        };
    }
    let key = wrap_cache_key(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    if let Some(cached) = wrap_cache().lock().unwrap().get(&key).cloned() {
        return cached;
    }
    let (ranges, truncated) = wrap_line_ranges_with_family(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    let lines = ranges
        .iter()
        .map(|(start, end)| text.get(*start..*end).unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    let result = (lines, truncated);
    wrap_cache().lock().unwrap().put(key, result.clone());
    result
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
    wrap_lines_with_family(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn wrap_lines_with_style(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<String>, bool) {
    wrap_lines_with_family(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn wrap_line_ranges_with_family(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<(usize, usize)>, bool) {
    if text.is_empty() {
        return if max_lines == Some(0) {
            (Vec::new(), true)
        } else {
            (vec![(0, 0)], false)
        };
    }
    if max_width <= 0.0 && soft_wrap {
        return if max_lines == Some(0) {
            (Vec::new(), true)
        } else {
            (vec![(0, 0)], false)
        };
    }
    let key = wrap_cache_key(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    if let Some(cached) = wrap_ranges_cache().lock().unwrap().get(&key).cloned() {
        return cached;
    }

    if !soft_wrap {
        let lines = hard_lines(text);
        let count = max_lines.map_or(lines.len(), |limit| limit.min(lines.len()));
        let truncated = count < lines.len();
        let result = (
            lines
                .into_iter()
                .take(count)
                .map(|line| (line.start, line.end))
                .collect(),
            truncated,
        );
        wrap_ranges_cache().lock().unwrap().put(key, result.clone());
        return result;
    }

    let mut output = Vec::new();
    let mut truncated = false;
    let mut eng = engine().lock().unwrap();
    for line in hard_lines(text) {
        if max_lines.is_some_and(|limit| output.len() >= limit) {
            truncated = output.len() < hard_lines(text).len();
            break;
        }
        let remaining = max_lines.map(|limit| limit.saturating_sub(output.len()));
        let segment = &text[line.start..line.end];
        let measurement = measure_line_with_engine(
            &mut eng,
            segment,
            px,
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
        );
        let (ranges, segment_truncated) =
            wrap_one_hard_line_ranges(segment, max_width, remaining, &measurement);
        output.extend(
            ranges
                .into_iter()
                .map(|(start, end)| (line.start + start, line.start + end)),
        );
        if segment_truncated {
            truncated = true;
            break;
        }
        if max_lines.is_some_and(|limit| output.len() >= limit) && line.next_start < text.len() {
            truncated = true;
            break;
        }
    }
    drop(eng);
    let result = (output, truncated);
    wrap_ranges_cache().lock().unwrap().put(key, result.clone());
    result
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
    wrap_line_ranges_with_family(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn wrap_line_ranges_with_style(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> (Vec<(usize, usize)>, bool) {
    wrap_line_ranges_with_family(
        text,
        px,
        max_width,
        max_lines,
        soft_wrap,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn ellipsize_line_with_family(
    text: &str,
    px: f32,
    max_width: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    let key = EllipCacheKey {
        text: text.to_owned(),
        px_bits: px.to_bits(),
        max_width_bits: max_width.to_bits(),
        family: font_family.map(str::to_owned),
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(str::to_owned),
        generation: font_generation(),
    };
    if let Some(cached) = ellip_cache().lock().unwrap().get(&key).cloned() {
        return cached;
    }
    let measurement = {
        let mut eng = engine().lock().unwrap();
        measure_line_with_engine(
            &mut eng,
            text,
            px,
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
        )
    };
    if visual_width(&measurement, 0, text.len()) <= max_width + 0.5 {
        let result = text.to_owned();
        ellip_cache().lock().unwrap().put(key, result.clone());
        return result;
    }
    let ellipsis_width = ellipsis_width_with_style(
        px,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
    if ellipsis_width >= max_width {
        return String::new();
    }
    let mut byte = 0usize;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let candidate = offset + grapheme.len();
        if containing_atomic_range(&measurement.atomic_ranges, candidate).is_some() {
            continue;
        }
        if visual_width(&measurement, 0, candidate) + ellipsis_width <= max_width + 0.5 {
            byte = candidate;
        }
    }
    let mut result = String::with_capacity(byte + '…'.len_utf8());
    result.push_str(&text[..byte]);
    result.push('…');
    ellip_cache().lock().unwrap().put(key, result.clone());
    result
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
    ellipsize_line_with_family(
        text,
        px,
        max_width,
        None,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

pub fn ellipsize_line_with_style(
    text: &str,
    px: f32,
    max_width: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> String {
    ellipsize_line_with_family(
        text,
        px,
        max_width,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    )
}

fn ellipsis_width_with_style(
    px: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> f32 {
    static ELLIP_W_LRU: OnceLock<
        Mutex<Lru<(u32, Option<String>, u16, u8, u32, Option<String>, u64), f32>>,
    > = OnceLock::new();
    static ELLIP_W_GEN: AtomicU64 = AtomicU64::new(0);
    let cache = ELLIP_W_LRU.get_or_init(|| Mutex::new(Lru::new(64)));
    let generation = font_generation();
    if ELLIP_W_GEN.load(Ordering::Relaxed) != generation {
        if let Ok(mut guard) = cache.lock() {
            guard.clear_both();
        }
        ELLIP_W_GEN.store(generation, Ordering::Relaxed);
    }
    let key = (
        px.to_bits(),
        font_family.map(str::to_owned),
        font_weight,
        font_style,
        letter_spacing.to_bits(),
        font_variation_settings.map(str::to_owned),
        generation,
    );
    if let Some(width) = cache.lock().unwrap().get(&key).copied() {
        return width;
    }
    let width = {
        let mut eng = engine().lock().unwrap();
        let measurement = measure_line_with_engine(
            &mut eng,
            "…",
            px,
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
        );
        visual_width(&measurement, 0, "…".len())
    };
    cache.lock().unwrap().put(key, width);
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bidi_caret_positions_follow_visual_order() {
        let text = "אבג";
        let metrics = metrics_for_textfield(text, 32.0, None, 400, 0, 0.0, None);
        assert!(metrics.positions.windows(2).all(|w| w[0] > w[1]));
        assert!(
            metrics
                .positions
                .iter()
                .all(|position| position.is_finite())
        );

        let mixed = "אב abc";
        let metrics = metrics_for_textfield(mixed, 32.0, None, 400, 0, 0.0, None);
        assert!(metrics.positions[0] > metrics.positions[1]);
        assert!(metrics.positions[1] > metrics.positions[2]);
        assert!(metrics.positions[4] < metrics.positions[5]);
        assert!(metrics.positions[5] < metrics.positions[6]);
    }

    #[test]
    fn utf8_layout_and_wrapping_do_not_panic() {
        let text = "e\u{301} 👩‍👩‍👧‍👦 🇺🇸 אב\u{00A0}界";
        let result = std::panic::catch_unwind(|| {
            let _ = metrics_for_textfield(text, 16.0, None, 400, 0, 0.0, None);
            let _ = wrap_line_ranges_with_family(
                text,
                16.0,
                12.0,
                Some(2),
                true,
                None,
                400,
                0,
                0.0,
                None,
            );
        });
        assert!(result.is_ok());
    }
}
