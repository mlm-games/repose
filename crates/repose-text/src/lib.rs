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
    collections::{BTreeMap, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
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
const GLYPH_CACHE_BYTES_CAP: usize = 64 * 1024 * 1024;
const MAX_RASTER_BYTES: usize = 16 * 1024 * 1024;
const WRAP_CACHE_CAP: usize = 1024;
const ELLIP_CACHE_CAP: usize = 2048;
const SHAPE_CACHE_CAP: usize = 1024;
const LEGACY_SHAPE_CACHE_CAP: usize = 512;
const RASTER_MIN_PX: f32 = 0.25;
const RASTER_MAX_PX: f32 = 4096.0;
const RASTER_QUANTUM: f32 = 1.0 / 4.0;
const MAX_RASTER_DIMENSION: u32 = 4_096;
const VERTICAL_METRICS_CACHE_CAP: usize = 256;
const OUTLINE_CACHE_CAP: usize = 2048;

#[derive(Hash, Eq, PartialEq)]
struct MetricsCacheKey {
    text: Arc<str>,
    px_bits: u32,
    family: Option<Arc<str>>,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<Arc<str>>,
    generation: u64,
}

#[derive(Hash, Eq, PartialEq)]
struct WrapCacheKey {
    text: Arc<str>,
    px_bits: u32,
    max_width_bits: u32,
    max_lines: Option<usize>,
    family: Option<Arc<str>>,
    soft_wrap: bool,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<Arc<str>>,
    generation: u64,
}

#[derive(Hash, Eq, PartialEq)]
struct EllipCacheKey {
    text: Arc<str>,
    px_bits: u32,
    max_width_bits: u32,
    family: Option<Arc<str>>,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<Arc<str>>,
    generation: u64,
}

static METRICS_LRU: OnceLock<Mutex<Lru<MetricsCacheKey, Arc<TextMetrics>>>> = OnceLock::new();
fn metrics_cache() -> &'static Mutex<Lru<MetricsCacheKey, Arc<TextMetrics>>> {
    METRICS_LRU.get_or_init(|| Mutex::new(Lru::new(4096)))
}

struct LruNode<K, V> {
    key: Arc<K>,
    value: V,
    newer: Option<usize>,
    older: Option<usize>,
}

struct Lru<K, V> {
    map: RapidHashMap<Arc<K>, usize>,
    nodes: Vec<Option<LruNode<K, V>>>,
    free: Vec<usize>,
    head: Option<usize>,
    tail: Option<usize>,
    cap: usize,
    len: usize,
}

impl<K: std::hash::Hash + Eq, V> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            map: RapidHashMap::new(),
            nodes: Vec::new(),
            free: Vec::new(),
            head: None,
            tail: None,
            cap,
            len: 0,
        }
    }

    fn node(&self, index: usize) -> Option<&LruNode<K, V>> {
        self.nodes.get(index).and_then(Option::as_ref)
    }

    fn node_mut(&mut self, index: usize) -> Option<&mut LruNode<K, V>> {
        self.nodes.get_mut(index).and_then(Option::as_mut)
    }

    fn unlink(&mut self, index: usize) {
        let Some((newer, older)) = self.node(index).map(|node| (node.newer, node.older)) else {
            return;
        };
        if let Some(newer) = newer {
            if let Some(node) = self.node_mut(newer) {
                node.older = older;
            }
        } else {
            self.head = older;
        }
        if let Some(older) = older {
            if let Some(node) = self.node_mut(older) {
                node.newer = newer;
            }
        } else {
            self.tail = newer;
        }
        if let Some(node) = self.node_mut(index) {
            node.newer = None;
            node.older = None;
        }
    }

    fn link_head(&mut self, index: usize) {
        let head = self.head;
        if let Some(node) = self.node_mut(index) {
            node.newer = None;
            node.older = head;
        }
        if let Some(head) = self.head
            && let Some(node) = self.node_mut(head)
        {
            node.newer = Some(index);
        }
        self.head = Some(index);
        if self.tail.is_none() {
            self.tail = Some(index);
        }
    }

    fn touch(&mut self, index: usize) {
        if self.head == Some(index) {
            return;
        }
        self.unlink(index);
        self.link_head(index);
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        let index = *self.map.get(key)?;
        self.touch(index);
        self.node(index).map(|node| &node.value)
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let index = *self.map.get(key)?;
        self.touch(index);
        self.node_mut(index).map(|node| &mut node.value)
    }

    fn put(&mut self, key: K, value: V) {
        if let Some(index) = self.map.get(&key).copied() {
            if let Some(node) = self.node_mut(index) {
                node.value = value;
            }
            self.touch(index);
            return;
        }
        if self.cap == 0 {
            return;
        }
        while self.len >= self.cap
            && let Some(victim) = self.tail
        {
            self.remove_index(victim);
        }
        let key = Arc::new(key);
        let index = if let Some(index) = self.free.pop() {
            self.nodes[index] = Some(LruNode {
                key: key.clone(),
                value,
                newer: None,
                older: None,
            });
            index
        } else {
            let index = self.nodes.len();
            self.nodes.push(Some(LruNode {
                key: key.clone(),
                value,
                newer: None,
                older: None,
            }));
            index
        };
        self.map.insert(key, index);
        self.link_head(index);
        self.len += 1;
    }

    fn remove_index(&mut self, index: usize) {
        let Some(node) = self.nodes.get_mut(index).and_then(Option::as_mut) else {
            return;
        };
        let key = node.key.clone();
        self.unlink(index);
        self.nodes[index] = None;
        self.map.remove(&key);
        self.free.push(index);
        self.len = self.len.saturating_sub(1);
    }

    fn clear_both(&mut self) {
        self.map.clear();
        self.nodes.clear();
        self.free.clear();
        self.head = None;
        self.tail = None;
        self.len = 0;
    }
}

static WRAP_LRU: OnceLock<Mutex<Lru<WrapCacheKey, Arc<(Vec<String>, bool)>>>> = OnceLock::new();

static WRAP_RANGES_LRU: OnceLock<Mutex<Lru<WrapCacheKey, Arc<(Vec<(usize, usize)>, bool)>>>> =
    OnceLock::new();

static ELLIP_LRU: OnceLock<Mutex<Lru<EllipCacheKey, Arc<String>>>> = OnceLock::new();

fn wrap_cache() -> &'static Mutex<Lru<WrapCacheKey, Arc<(Vec<String>, bool)>>> {
    WRAP_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn wrap_ranges_cache() -> &'static Mutex<Lru<WrapCacheKey, Arc<(Vec<(usize, usize)>, bool)>>> {
    WRAP_RANGES_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}

fn ellip_cache() -> &'static Mutex<Lru<EllipCacheKey, Arc<String>>> {
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

#[derive(Clone)]
pub struct ShapedGlyph {
    pub key: GlyphKey,
    pub cache_key: CacheKey,
    pub font_id: u64,
    pub glyph_id: u32,
    pub px: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub rasterized: bool,
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
    data_bytes: Arc<[u8]>,
    face_index: u32,
}

struct Engine {
    font_cx: parley::FontContext,
    layout_cx: parley::LayoutContext<()>,
    swash_cx: swash::scale::ScaleContext,
    key_map: RapidHashMap<GlyphKey, (u64, u32)>,
    font_registry: Vec<FontRecord>,
    font_registry_index: RapidHashMap<(u64, u32), u64>,
    next_font_id: u64,
    /// Cache of rendered glyphs keyed by (font_id, glyph_id, font_size_bits).
    /// Contains (width, height, left, top, content, data).
    glyph_cache:
        RapidHashMap<(u64, u32, u32), (u32, u32, i32, i32, swash::scale::image::Content, Vec<u8>)>,
    glyph_cache_bytes: usize,
    outline_cache: Lru<(u64, u32), Arc<[Command]>>,
    /// Cache of (ascent, descent) in px keyed by
    /// (family hash, weight, px bits). Used for baseline alignment.
    ascent_cache: RapidHashMap<(Option<Arc<str>>, u16, u32, u64), (f32, f32)>,
}

fn normalized_raster_px(px: f32) -> Option<f32> {
    if !px.is_finite() || px <= 0.0 {
        return None;
    }
    let clamped = px.clamp(RASTER_MIN_PX, RASTER_MAX_PX);
    let quantized = (clamped / RASTER_QUANTUM).round() * RASTER_QUANTUM;
    Some(quantized.clamp(RASTER_MIN_PX, RASTER_MAX_PX))
}

impl Engine {
    fn ensure_font(&mut self, fd: &parley::FontData) -> u64 {
        let blob_id = fd.data.id();
        let lookup_key = (blob_id, fd.index);
        if let Some(id) = self.font_registry_index.get(&lookup_key).copied() {
            return id;
        }
        if let Some(existing) = self.font_registry.iter().find(|record| {
            record.face_index == fd.index && record.data_bytes.as_ref() == fd.data.as_ref()
        }) {
            self.font_registry_index.insert(lookup_key, existing.id);
            return existing.id;
        }
        let id = self.next_font_id;
        self.next_font_id += 1;
        let bytes: Arc<[u8]> = Arc::from(fd.data.as_ref());
        log::debug!("[font] register id={} len={}", id, bytes.len());
        self.font_registry.push(FontRecord {
            id,
            data_bytes: bytes,
            face_index: fd.index,
        });
        self.font_registry_index.insert(lookup_key, id);
        id
    }

    fn trim_glyph_cache(&mut self) {
        while self.glyph_cache.len() > GLYPH_CACHE_CAP
            || self.glyph_cache_bytes > GLYPH_CACHE_BYTES_CAP
        {
            let Some((key, value)) = self.glyph_cache.iter().next() else {
                break;
            };
            let key = *key;
            let bytes = value.5.len();
            self.glyph_cache.remove(&key);
            self.glyph_cache_bytes = self.glyph_cache_bytes.saturating_sub(bytes);
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
        let px = px.clamp(RASTER_MIN_PX, RASTER_MAX_PX);
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
        let mut best: Option<(f32, Arc<[u8]>, u32)> = None;
        for font in info.fonts() {
            let dist = (font.weight().value() - target).abs();
            if best.as_ref().is_some_and(|(bd, _, _)| *bd <= dist) {
                continue;
            }
            let bytes: Arc<[u8]> = match font.source().kind() {
                parley::fontique::SourceKind::Memory(blob) => Arc::from(blob.as_ref()),
                #[cfg(not(target_arch = "wasm32"))]
                parley::fontique::SourceKind::Path(path) => Arc::from(std::fs::read(path).ok()?),
                #[cfg(target_arch = "wasm32")]
                parley::fontique::SourceKind::Path(_) => continue,
            };
            best = Some((dist, bytes, font.index()));
        }
        let (_, bytes, index) = best?;
        let font = skrifa::FontRef::from_index(bytes.as_ref(), index).ok()?;
        let metrics = font.metrics(
            skrifa::instance::Size::new(px),
            skrifa::instance::LocationRef::default(),
        );
        let limit = px * 8.0;
        let ascent = if metrics.ascent.is_finite() {
            metrics.ascent.max(0.0).min(limit)
        } else {
            0.0
        };
        let descent = if metrics.descent.is_finite() {
            metrics.descent.abs().max(0.0).min(limit)
        } else {
            0.0
        };
        Some((ascent, descent))
    }

    fn raster_placement(
        &mut self,
        font_id: u64,
        glyph_id: u32,
        px: f32,
    ) -> Option<(f32, f32, f32, f32)> {
        use swash::scale::{Render, Source, StrikeWith};
        let px = normalized_raster_px(px)?;
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
        let font = swash::FontRef::from_index(data_bytes.as_ref(), face_index)?;
        let mut scaler = self.swash_cx.builder(font).size(px).hint(true).build();
        let image = Render::new(&[
            Source::Outline,
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::ColorOutline(0),
        ])
        .render(&mut scaler, swash_glyph_id(font_id, glyph_id))?;
        if image.placement.width > MAX_RASTER_DIMENSION
            || image.placement.height > MAX_RASTER_DIMENSION
            || image.data.len() > MAX_RASTER_BYTES
        {
            return None;
        }
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
        let width = image.placement.width;
        let height = image.placement.height;
        let left = image.placement.left;
        let top = image.placement.top;
        let data_len = image.data.len();
        let previous = self.glyph_cache.insert(
            cache_key,
            (width, height, left, top, image.content, image.data),
        );
        if let Some(previous) = previous {
            self.glyph_cache_bytes = self.glyph_cache_bytes.saturating_sub(previous.5.len());
        }
        self.glyph_cache_bytes = self.glyph_cache_bytes.saturating_add(data_len);
        self.trim_glyph_cache();
        Some((width as f32, height as f32, left as f32, top as f32))
    }
}

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

pub static FONT_PROVIDER: OnceLock<Mutex<font_awl::Provider>> = OnceLock::new();
#[cfg(target_arch = "wasm32")]
static RETAINED_FONT_DATA: OnceLock<Mutex<Vec<Arc<[u8]>>>> = OnceLock::new();

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

fn font_blob(bytes: Arc<[u8]>) -> parley::fontique::Blob<u8> {
    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes);
    parley::fontique::Blob::new(data)
}

fn register_asset_if_missing(provider: &mut font_awl::Provider, bytes: &[u8]) {
    if collection_font_data_families(provider.collection_mut(), bytes).is_empty() {
        let blob = font_blob(Arc::from(bytes));
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
        key_map: RapidHashMap::new(),
        font_registry: Vec::new(),
        font_registry_index: RapidHashMap::new(),
        next_font_id: 1,
        glyph_cache: RapidHashMap::new(),
        glyph_cache_bytes: 0,
        outline_cache: Lru::new(OUTLINE_CACHE_CAP),
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
fn retain_font_data(bytes: Arc<[u8]>) {
    let retained = RETAINED_FONT_DATA.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut retained) = retained.lock() else {
        return;
    };
    if retained
        .iter()
        .all(|existing| existing.as_ref() != bytes.as_ref())
    {
        retained.push(bytes);
    }
}

#[cfg(target_arch = "wasm32")]
fn restore_retained_font_data(collection: &mut parley::fontique::Collection) {
    let retained = RETAINED_FONT_DATA.get().and_then(|retained| {
        retained
            .lock()
            .ok()
            .map(|retained| retained.as_slice().to_vec())
    });
    let Some(retained) = retained else {
        return;
    };
    for bytes in retained {
        if collection_font_data_families(collection, bytes.as_ref()).is_empty() {
            let blob = font_blob(bytes);
            let families = collection.register_fonts(blob, None);
            append_registered_families(collection, &families);
        }
    }
}

pub(crate) fn register_font_data_if_usable(bytes: &[u8]) -> bool {
    #[cfg(target_arch = "wasm32")]
    let mut retained_bytes = None;
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
            let shared_bytes: Arc<[u8]> = Arc::from(bytes);
            let blob = font_blob(shared_bytes.clone());
            let families = p.collection_mut().register_fonts(blob, None);
            if families.is_empty() {
                return false;
            }
            #[cfg(target_arch = "wasm32")]
            {
                retained_bytes = Some(shared_bytes);
            }
            append_registered_families(p.collection_mut(), &families);
            configure_collection(p.collection_mut());
            (p.new_parley_context(), true)
        }
    };

    if newly_registered {
        #[cfg(target_arch = "wasm32")]
        if let Some(bytes) = retained_bytes {
            retain_font_data(bytes);
        }
        let mut eng = engine().lock().unwrap();
        eng.font_cx = font_cx;
        clear_caches_for_fallback_in(&mut eng);
    }

    true
}

pub(crate) fn clear_caches_for_fallback_in(eng: &mut Engine) {
    clear_lru_caches();
    eng.key_map.clear();
    eng.font_registry.clear();
    eng.font_registry_index.clear();
    eng.glyph_cache.clear();
    eng.glyph_cache_bytes = 0;
    eng.outline_cache.clear_both();
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
    if let Some(c) = SHAPED_LRU.get()
        && let Ok(mut g) = c.lock()
    {
        g.clear_both();
    }
    if let Some(c) = LEGACY_SHAPED_LRU.get()
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
    let Some(px) = normalized_raster_px(px) else {
        return (0.0, 0.0);
    };
    let key = (
        font_family.map(Arc::from),
        font_weight,
        px.to_bits(),
        font_generation(),
    );
    let mut eng = engine().lock().unwrap();
    if let Some(&m) = eng.ascent_cache.get(&key) {
        return m;
    }
    let m = eng.resolve_vertical_metrics(font_family, font_weight, px);
    if m.0.is_finite() && m.1.is_finite() && eng.ascent_cache.len() >= VERTICAL_METRICS_CACHE_CAP {
        eng.ascent_cache.clear();
    }
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
                let mut glyph_count = 0usize;
                let mut has_missing = false;
                for glyph in cluster.glyphs() {
                    glyph_count += 1;
                    has_missing |= glyph.id == 0;
                }
                total_glyphs += glyph_count;
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

    let raster_px = normalized_raster_px(px).unwrap_or(0.0);
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
            let run_cache_key = CacheKey {
                font_id: fid,
                glyph_id: 0,
                font_size_bits: raster_px.to_bits(),
            };
            log::debug!(
                "[shape] run: fid={} font_data_len={}",
                fid,
                font_data.data.as_ref().len()
            );
            for glyph in glyph_run.positioned_glyphs() {
                let gid = glyph.id;
                let key = key_from_pair(fid, gid);
                eng.key_map.insert(key, (fid, gid));
                let cache_key = CacheKey {
                    glyph_id: gid,
                    ..run_cache_key
                };
                glyphs.push(ShapedGlyph {
                    key,
                    cache_key,
                    font_id: fid,
                    glyph_id: gid,
                    px,
                    x: glyph.x,
                    y: glyph.y,
                    w: 0.0,
                    h: 0.0,
                    bearing_x: 0.0,
                    bearing_y: 0.0,
                    advance: glyph.advance,
                    rasterized: false,
                });
            }
        }
    }
    (glyphs, runs)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
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

#[derive(Clone)]
pub struct ShapedRun {
    pub text_range: std::ops::Range<usize>,
    pub rtl: bool,
    pub synthesis: parley::fontique::Synthesis,
}

#[derive(Clone)]
pub struct ShapedText {
    pub glyphs: Vec<ShapedGlyph>,
    pub requested_text_direction: TextDirection,
    pub resolved_text_direction: TextDirection,
    pub runs: Vec<ShapedRun>,
}

#[derive(Clone)]
pub struct SharedShapedText {
    pub glyphs: Arc<[ShapedGlyph]>,
    pub requested_text_direction: TextDirection,
    pub resolved_text_direction: TextDirection,
    pub runs: Arc<[ShapedRun]>,
}

pub type CachedShapedText = SharedShapedText;
pub type VectorShapedText = SharedShapedText;
pub type VectorShapedLine = SharedShapedText;
pub type ShapedLine = SharedShapedText;

#[derive(Clone)]
struct ShapeCacheKey {
    text: Arc<str>,
    px_bits: u32,
    line_height_bits: u32,
    family: Option<Arc<str>>,
    font_weight: u16,
    font_style: u8,
    letter_spacing_bits: u32,
    variation: Option<Arc<str>>,
    generation: u64,
    text_direction: TextDirection,
    font_synthesis: FontSynthesis,
}

impl ShapeCacheKey {
    fn new(
        text: &str,
        px: f32,
        line_height_ratio: f32,
        options: ShapeOptions<'_>,
        generation: u64,
    ) -> Self {
        Self {
            text: Arc::from(text),
            px_bits: px.to_bits(),
            line_height_bits: line_height_ratio.to_bits(),
            family: options.font_family.map(Arc::from),
            font_weight: options.font_weight,
            font_style: options.font_style,
            letter_spacing_bits: options.letter_spacing.to_bits(),
            variation: options.font_variation_settings.map(Arc::from),
            generation,
            text_direction: options.text_direction,
            font_synthesis: options.font_synthesis,
        }
    }

    fn matches(
        &self,
        text: &str,
        px: f32,
        line_height_ratio: f32,
        options: ShapeOptions<'_>,
    ) -> bool {
        self.text.as_ref() == text
            && self.px_bits == px.to_bits()
            && self.line_height_bits == line_height_ratio.to_bits()
            && self.family.as_deref() == options.font_family
            && self.font_weight == options.font_weight
            && self.font_style == options.font_style
            && self.letter_spacing_bits == options.letter_spacing.to_bits()
            && self.variation.as_deref() == options.font_variation_settings
            && self.text_direction == options.text_direction
            && self.font_synthesis == options.font_synthesis
    }
}

struct ShapeCacheEntry {
    key: ShapeCacheKey,
    value: Arc<SharedShapedText>,
}

static SHAPED_LRU: OnceLock<Mutex<Lru<u64, Vec<ShapeCacheEntry>>>> = OnceLock::new();

struct LegacyShapeCacheEntry {
    key: ShapeCacheKey,
    value: Arc<[ShapedGlyph]>,
}

static LEGACY_SHAPED_LRU: OnceLock<Mutex<Lru<u64, Vec<LegacyShapeCacheEntry>>>> = OnceLock::new();

fn shaped_cache() -> &'static Mutex<Lru<u64, Vec<ShapeCacheEntry>>> {
    SHAPED_LRU.get_or_init(|| Mutex::new(Lru::new(SHAPE_CACHE_CAP)))
}

fn legacy_shaped_cache() -> &'static Mutex<Lru<u64, Vec<LegacyShapeCacheEntry>>> {
    LEGACY_SHAPED_LRU.get_or_init(|| Mutex::new(Lru::new(LEGACY_SHAPE_CACHE_CAP)))
}

fn shape_fingerprint(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
    generation: u64,
) -> u64 {
    let mut hasher = RapidHasher::default();
    text.hash(&mut hasher);
    px.to_bits().hash(&mut hasher);
    line_height_ratio.to_bits().hash(&mut hasher);
    options.font_family.hash(&mut hasher);
    options.font_weight.hash(&mut hasher);
    options.font_style.hash(&mut hasher);
    options.letter_spacing.to_bits().hash(&mut hasher);
    options.font_variation_settings.hash(&mut hasher);
    options.text_direction.hash(&mut hasher);
    options.font_synthesis.hash(&mut hasher);
    generation.hash(&mut hasher);
    hasher.finish()
}

fn cached_shaped(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
    generation: u64,
    fingerprint: u64,
) -> Option<Arc<SharedShapedText>> {
    let mut cache = shaped_cache().lock().ok()?;
    let entries = cache.get_mut(&fingerprint)?;
    let entry = entries.iter().find(|entry| {
        entry.key.generation == generation
            && entry.key.matches(text, px, line_height_ratio, options)
    })?;
    Some(entry.value.clone())
}

fn cache_shaped(fingerprint: u64, key: ShapeCacheKey, value: Arc<SharedShapedText>) {
    let mut cache = shaped_cache().lock().unwrap();
    if let Some(entries) = cache.get_mut(&fingerprint) {
        if let Some(entry) = entries.iter_mut().find(|entry| {
            entry.key.text == key.text
                && entry.key.px_bits == key.px_bits
                && entry.key.line_height_bits == key.line_height_bits
                && entry.key.family == key.family
                && entry.key.font_weight == key.font_weight
                && entry.key.font_style == key.font_style
                && entry.key.letter_spacing_bits == key.letter_spacing_bits
                && entry.key.variation == key.variation
                && entry.key.text_direction == key.text_direction
                && entry.key.font_synthesis == key.font_synthesis
        }) {
            entry.key.generation = key.generation;
            entry.value = value;
            return;
        }
        entries.push(ShapeCacheEntry { key, value });
        return;
    }
    cache.put(fingerprint, vec![ShapeCacheEntry { key, value }]);
}

fn cached_legacy_glyphs(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
    generation: u64,
    fingerprint: u64,
) -> Option<Arc<[ShapedGlyph]>> {
    let mut cache = legacy_shaped_cache().lock().ok()?;
    let entries = cache.get_mut(&fingerprint)?;
    let entry = entries.iter().find(|entry| {
        entry.key.generation == generation
            && entry.key.matches(text, px, line_height_ratio, options)
    })?;
    Some(entry.value.clone())
}

fn cache_legacy_glyphs(fingerprint: u64, key: ShapeCacheKey, value: Arc<[ShapedGlyph]>) {
    let mut cache = legacy_shaped_cache().lock().unwrap();
    if let Some(entries) = cache.get_mut(&fingerprint) {
        if let Some(entry) = entries.iter_mut().find(|entry| {
            entry.key.text == key.text
                && entry.key.px_bits == key.px_bits
                && entry.key.line_height_bits == key.line_height_bits
                && entry.key.family == key.family
                && entry.key.font_weight == key.font_weight
                && entry.key.font_style == key.font_style
                && entry.key.letter_spacing_bits == key.letter_spacing_bits
                && entry.key.variation == key.variation
                && entry.key.text_direction == key.text_direction
                && entry.key.font_synthesis == key.font_synthesis
        }) {
            entry.key.generation = key.generation;
            entry.value = value;
            return;
        }
        entries.push(LegacyShapeCacheEntry { key, value });
        return;
    }
    cache.put(fingerprint, vec![LegacyShapeCacheEntry { key, value }]);
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

fn shape_vector_inner(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
    collect_runs: bool,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    let generation = font_generation();
    let fingerprint = shape_fingerprint(text, px, line_height_ratio, options, generation);
    if let Some(value) = cached_shaped(
        text,
        px,
        line_height_ratio,
        options,
        generation,
        fingerprint,
    ) {
        return Ok(value);
    }
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
    let (glyphs, runs) = collect_shaped_layout(&mut eng, layout, px, collect_runs);
    validate_shape_options(options, resolved_direction, &runs)?;
    let value = Arc::new(SharedShapedText {
        glyphs: Arc::from(glyphs.into_boxed_slice()),
        requested_text_direction: options.text_direction,
        resolved_text_direction: resolved_direction,
        runs: Arc::from(runs.into_boxed_slice()),
    });
    let key = ShapeCacheKey::new(text, px, line_height_ratio, options, generation);
    drop(eng);
    cache_shaped(fingerprint, key, value.clone());
    Ok(value)
}

fn materialize_legacy_glyphs(
    shaped: &SharedShapedText,
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Vec<ShapedGlyph> {
    let generation = font_generation();
    let fingerprint = shape_fingerprint(text, px, line_height_ratio, options, generation);
    if let Some(cached) = cached_legacy_glyphs(
        text,
        px,
        line_height_ratio,
        options,
        generation,
        fingerprint,
    ) {
        return cached.as_ref().to_vec();
    }
    let mut glyphs = shaped.glyphs.as_ref().to_vec();
    let mut eng = engine().lock().unwrap();
    for glyph in &mut glyphs {
        if let Some((width, height, left, top)) =
            eng.raster_placement(glyph.font_id, glyph.glyph_id, glyph.px)
        {
            glyph.w = width;
            glyph.h = height;
            glyph.bearing_x = left;
            glyph.bearing_y = top;
            glyph.rasterized = true;
        }
    }
    drop(eng);
    let value: Arc<[ShapedGlyph]> = Arc::from(glyphs.into_boxed_slice());
    cache_legacy_glyphs(
        fingerprint,
        ShapeCacheKey::new(text, px, line_height_ratio, options, generation),
        value.clone(),
    );
    value.as_ref().to_vec()
}

pub fn shape_line_vector(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Arc<SharedShapedText> {
    shape_vector_inner(
        text,
        px,
        line_height_ratio,
        ShapeOptions {
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
            ..ShapeOptions::default()
        },
        true,
    )
    .unwrap_or_else(|_| {
        Arc::new(SharedShapedText {
            glyphs: Arc::from(Vec::new()),
            requested_text_direction: TextDirection::Auto,
            resolved_text_direction: TextDirection::Ltr,
            runs: Arc::from(Vec::new()),
        })
    })
}

pub fn shape_line_cached(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Arc<SharedShapedText> {
    shape_line_vector(
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

pub fn shape_line_shared(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Arc<SharedShapedText> {
    shape_line_vector(
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

pub fn shape_text_with_options_vector(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    shape_vector_inner(text, px, line_height_ratio, options, true)
}

pub fn shape_text_with_options_shared(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    shape_text_with_options_vector(text, px, line_height_ratio, options)
}

pub fn shape_text_with_options_cached(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    shape_text_with_options_vector(text, px, line_height_ratio, options)
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
    let options = ShapeOptions {
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
        ..ShapeOptions::default()
    };
    materialize_legacy_glyphs(
        &shape_line_vector(
            text,
            px,
            line_height_ratio,
            font_family,
            font_weight,
            font_style,
            letter_spacing,
            font_variation_settings,
        ),
        text,
        px,
        line_height_ratio,
        options,
    )
}

pub fn shape_text_with_options(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<ShapedText, ShapeCapabilityError> {
    let shaped = shape_text_with_options_vector(text, px, line_height_ratio, options)?;
    Ok(ShapedText {
        glyphs: materialize_legacy_glyphs(&shaped, text, px, line_height_ratio, options),
        requested_text_direction: shaped.requested_text_direction,
        resolved_text_direction: shaped.resolved_text_direction,
        runs: shaped.runs.as_ref().to_vec(),
    })
}

pub fn shape_line_with_options_vector(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    shape_text_with_options_vector(text, px, line_height_ratio, options)
}

pub fn shape_line_with_options_cached(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Arc<SharedShapedText>, ShapeCapabilityError> {
    shape_line_with_options_vector(text, px, line_height_ratio, options)
}

pub fn shape_line_with_options(
    text: &str,
    px: f32,
    line_height_ratio: f32,
    options: ShapeOptions<'_>,
) -> Result<Vec<ShapedGlyph>, ShapeCapabilityError> {
    shape_text_with_options(text, px, line_height_ratio, options).map(|shaped| shaped.glyphs)
}

fn rasterize_locked(
    eng: &mut Engine,
    key: GlyphKey,
    fid: u64,
    gid: u32,
    px: f32,
) -> Option<GlyphBitmap> {
    use swash::scale::{Render, Source, StrikeWith};
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
    let font = swash::FontRef::from_index(data_bytes.as_ref(), face_index)?;
    let mut scaler = eng.swash_cx.builder(font).size(px).hint(true).build();
    let image = Render::new(&[
        Source::Outline,
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::ColorOutline(0),
    ])
    .render(&mut scaler, swash_glyph_id(fid, gid))?;
    if image.placement.width > MAX_RASTER_DIMENSION
        || image.placement.height > MAX_RASTER_DIMENSION
        || image.data.len() > MAX_RASTER_BYTES
    {
        return None;
    }
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
    let data_len = bitmap.data.len();
    let previous = eng.glyph_cache.insert(
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
    if let Some(previous) = previous {
        eng.glyph_cache_bytes = eng.glyph_cache_bytes.saturating_sub(previous.5.len());
    }
    eng.glyph_cache_bytes = eng.glyph_cache_bytes.saturating_add(data_len);
    eng.trim_glyph_cache();
    Some(bitmap)
}

pub fn rasterize(key: GlyphKey, px: f32) -> Option<GlyphBitmap> {
    let mut eng = engine().lock().unwrap();
    let &(fid, gid) = eng.key_map.get(&key)?;
    let px = normalized_raster_px(px)?;
    rasterize_locked(&mut eng, key, fid, gid, px)
}

pub fn rasterize_cache_key(cache_key: CacheKey) -> Option<GlyphBitmap> {
    let px = f32::from_bits(cache_key.font_size_bits);
    let normalized = normalized_raster_px(px)?;
    let mut eng = engine().lock().unwrap();
    rasterize_locked(
        &mut eng,
        key_from_pair(cache_key.font_id, cache_key.glyph_id),
        cache_key.font_id,
        cache_key.glyph_id,
        normalized,
    )
}

pub fn raster_placement(cache_key: CacheKey) -> Option<(f32, f32, f32, f32)> {
    let px = f32::from_bits(cache_key.font_size_bits);
    let mut eng = engine().lock().unwrap();
    eng.raster_placement(cache_key.font_id, cache_key.glyph_id, px)
}

pub fn lookup_cache_key(key: GlyphKey, px: f32) -> Option<CacheKey> {
    let eng = engine().lock().unwrap();
    let &(fid, gid) = eng.key_map.get(&key)?;
    let px = normalized_raster_px(px)?;
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

pub fn extract_outline_commands_shared(cache_key: CacheKey) -> Option<Arc<[Command]>> {
    let cache_identity = (cache_key.font_id, cache_key.glyph_id);
    if let Some(cached) = engine()
        .lock()
        .unwrap()
        .outline_cache
        .get(&cache_identity)
        .cloned()
    {
        return Some(cached);
    }
    let (data_bytes, face_index) = {
        let eng = engine().lock().unwrap();
        let record = eng
            .font_registry
            .iter()
            .find(|record| record.id == cache_key.font_id)?;
        (record.data_bytes.clone(), record.face_index)
    };
    let commands = extract_outlines_for(data_bytes.as_ref(), face_index, cache_key.glyph_id)?;
    let commands: Arc<[Command]> = Arc::from(commands);
    engine()
        .lock()
        .unwrap()
        .outline_cache
        .put(cache_identity, commands.clone());
    Some(commands)
}

pub fn extract_outline_commands(cache_key: CacheKey) -> Option<Box<[Command]>> {
    extract_outline_commands_shared(cache_key)
        .map(|commands| commands.as_ref().to_vec().into_boxed_slice())
}

pub fn extract_outline_commands_for(cache_key: CacheKey) -> Option<Box<[Command]>> {
    extract_outline_commands(cache_key)
}

pub fn lookup_and_extract_outline(key: GlyphKey, px: f32) -> Option<(CacheKey, Box<[Command]>)> {
    let (fid, gid) = {
        let eng = engine().lock().unwrap();
        *eng.key_map.get(&key)?
    };
    let px = normalized_raster_px(px)?;
    let ck = CacheKey {
        font_id: fid,
        glyph_id: gid,
        font_size_bits: px.to_bits(),
    };
    let commands = extract_outline_commands_shared(ck)?;
    Some((ck, commands.as_ref().to_vec().into_boxed_slice()))
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

struct SegmentExtrema {
    len: usize,
    base: usize,
    min: Vec<f32>,
    max: Vec<f32>,
    prefix_min: Vec<f32>,
    prefix_max: Vec<f32>,
    has_prefix: bool,
}

impl SegmentExtrema {
    fn new(len: usize, has_prefix: bool) -> Self {
        let len = len.max(1);
        let base = len.next_power_of_two();
        Self {
            len,
            base,
            min: vec![f32::INFINITY; base * 2],
            max: vec![f32::NEG_INFINITY; base * 2],
            prefix_min: Vec::with_capacity(if has_prefix { len + 1 } else { 0 }),
            prefix_max: Vec::with_capacity(if has_prefix { len + 1 } else { 0 }),
            has_prefix,
        }
    }

    fn set(&mut self, index: usize, value: f32) {
        if value.is_finite() && index < self.len {
            self.min[self.base + index] = value;
            self.max[self.base + index] = value;
        }
    }

    fn apply(&mut self, start: usize, end: usize, min_value: f32, max_value: f32) {
        if start >= end {
            return;
        }
        let mut left = start.min(self.len) + self.base;
        let mut right = end.min(self.len) + self.base;
        while left < right {
            if left & 1 == 1 {
                self.min[left] = self.min[left].min(min_value);
                self.max[left] = self.max[left].max(max_value);
                left += 1;
            }
            if right & 1 == 1 {
                right -= 1;
                self.min[right] = self.min[right].min(min_value);
                self.max[right] = self.max[right].max(max_value);
            }
            left >>= 1;
            right >>= 1;
        }
    }

    fn finish(&mut self) {
        for index in 1..self.base {
            let min_value = self.min[index];
            let max_value = self.max[index];
            if min_value.is_finite() {
                self.min[index * 2] = self.min[index * 2].min(min_value);
                self.min[index * 2 + 1] = self.min[index * 2 + 1].min(min_value);
            }
            if max_value.is_finite() {
                self.max[index * 2] = self.max[index * 2].max(max_value);
                self.max[index * 2 + 1] = self.max[index * 2 + 1].max(max_value);
            }
        }
        for index in (1..self.base).rev() {
            self.min[index] = self.min[index * 2].min(self.min[index * 2 + 1]);
            self.max[index] = self.max[index * 2].max(self.max[index * 2 + 1]);
        }
        if self.has_prefix {
            self.prefix_min.push(f32::INFINITY);
            self.prefix_max.push(f32::NEG_INFINITY);
            for index in 0..self.len {
                let value_min = self.min[self.base + index];
                let value_max = self.max[self.base + index];
                let previous_min = *self.prefix_min.last().unwrap_or(&f32::INFINITY);
                let previous_max = *self.prefix_max.last().unwrap_or(&f32::NEG_INFINITY);
                self.prefix_min.push(previous_min.min(value_min));
                self.prefix_max.push(previous_max.max(value_max));
            }
        }
    }

    fn query(&self, start: usize, end: usize) -> (f32, f32) {
        let start = start.min(self.len);
        let end = end.min(self.len).max(start);
        if start == end {
            return (f32::INFINITY, f32::NEG_INFINITY);
        }
        if start == 0 && self.has_prefix {
            return (self.prefix_min[end], self.prefix_max[end]);
        }
        let mut left = start + self.base;
        let mut right = end + self.base;
        let mut min_value = f32::INFINITY;
        let mut max_value = f32::NEG_INFINITY;
        while left < right {
            if left & 1 == 1 {
                min_value = min_value.min(self.min[left]);
                max_value = max_value.max(self.max[left]);
                left += 1;
            }
            if right & 1 == 1 {
                right -= 1;
                min_value = min_value.min(self.min[right]);
                max_value = max_value.max(self.max[right]);
            }
            left >>= 1;
            right >>= 1;
        }
        (min_value, max_value)
    }
}

struct RangeExtremum {
    positions: SegmentExtrema,
    extents: SegmentExtrema,
    coordinates: Vec<usize>,
}

impl RangeExtremum {
    fn from_measurement(text: &str, metrics: &TextMetrics, extents: &[VisualExtent]) -> Self {
        let mut coordinates = metrics.byte_offsets.clone();
        coordinates.extend(text.char_indices().map(|(index, _)| index));
        coordinates.push(text.len());
        for extent in extents {
            coordinates.push(extent.start);
            coordinates.push(extent.end);
        }
        coordinates.sort_unstable();
        coordinates.dedup();
        let mut positions = SegmentExtrema::new(metrics.positions.len(), true);
        for (index, value) in metrics.positions.iter().copied().enumerate() {
            positions.set(index, value);
        }
        let mut range_extents = SegmentExtrema::new(coordinates.len().saturating_sub(1), false);
        for extent in extents {
            let left = extent.left.min(extent.right);
            let right = extent.left.max(extent.right);
            if !left.is_finite() || !right.is_finite() {
                continue;
            }
            let start = coordinates.partition_point(|byte| *byte < extent.start);
            let end = coordinates.partition_point(|byte| *byte < extent.end);
            range_extents.apply(start, end, left, right);
        }
        positions.finish();
        range_extents.finish();
        Self {
            positions,
            extents: range_extents,
            coordinates,
        }
    }

    fn query_bytes(&self, offsets: &[usize], start: usize, end: usize) -> (f32, f32) {
        let position_start = offsets.partition_point(|byte| *byte < start);
        let position_end = offsets.partition_point(|byte| *byte <= end);
        let (position_min, position_max) = self.positions.query(position_start, position_end);
        let extent_start = self.coordinates.partition_point(|byte| *byte < start);
        let extent_end = self.coordinates.partition_point(|byte| *byte < end);
        let (extent_min, extent_max) = self.extents.query(extent_start, extent_end);
        (position_min.min(extent_min), position_max.max(extent_max))
    }
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
    first_part: Option<LogicalPart>,
    extra_parts: Vec<LogicalPart>,
    has_ligature_start: bool,
    advance: f32,
}

impl LogicalGroup {
    fn parts(&self) -> impl Iterator<Item = &LogicalPart> {
        self.first_part.iter().chain(self.extra_parts.iter())
    }
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
    atomic_ranges: Vec<AtomicRange>,
    extrema: RangeExtremum,
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
    if let Some(part) = &mut group.first_part
        && !part.advance.is_finite()
    {
        part.advance = 0.0;
    }
    for part in &mut group.extra_parts {
        if !part.advance.is_finite() {
            part.advance = 0.0;
        }
    }
    group.advance = group.parts().map(|part| part.advance).sum();
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
                first_part: Some(part),
                extra_parts: Vec::new(),
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
            first_part: None,
            extra_parts: Vec::new(),
            has_ligature_start: false,
            advance: 0.0,
        });
        group.start = group.start.min(part.start);
        group.end = group.end.max(part.end);
        if group.first_part.is_none() {
            group.first_part = Some(part);
        } else {
            group.extra_parts.push(part);
        }
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
    let count = slice.graphemes(true).count();
    if count == 0 {
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
    candidates.push(EdgeCandidate {
        byte: start,
        position: start_x,
        kind: EdgeKind::Start,
    });
    for (index, (offset, grapheme)) in slice.grapheme_indices(true).enumerate() {
        let fraction = (index + 1) as f32 / count as f32;
        let position = start_x + (end_x - start_x) * fraction;
        let kind = if index + 1 == count {
            EdgeKind::End
        } else {
            EdgeKind::Internal
        };
        candidates.push(EdgeCandidate {
            byte: start + offset + grapheme.len(),
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
            let mut visual_clusters = HashMap::with_capacity(run.len());
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
                for part in group.parts() {
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
    let (min, max) = measurement
        .extrema
        .query_bytes(&measurement.metrics.byte_offsets, start, end);
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
    let extrema = RangeExtremum::from_measurement(text, &metrics, &layout_edges.extents);
    LineMeasurement {
        metrics,
        atomic_ranges: layout_edges.atomic_ranges,
        extrema,
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
    let hard_line_list = hard_lines(text);
    for line in &hard_line_list {
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
    for line in &hard_line_list {
        if line.next_start > line.end {
            edges.insert(line.next_start, 0.0);
        }
    }
    let edges = edges.into_iter().collect::<Vec<_>>();
    metrics_from_edges(text, &edges)
}

pub fn metrics_for_textfield_shared(
    text: &str,
    px: f32,
    font_family: Option<&str>,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> Arc<TextMetrics> {
    let key = MetricsCacheKey {
        text: Arc::from(text),
        px_bits: px.to_bits(),
        family: font_family.map(Arc::from),
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(Arc::from),
        generation: font_generation(),
    };
    if let Some(metrics) = metrics_cache().lock().unwrap().get(&key).cloned() {
        return metrics;
    }
    let metrics = {
        let mut eng = engine().lock().unwrap();
        measure_text_with_engine(
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
    let metrics = Arc::new(metrics);
    metrics_cache().lock().unwrap().put(key, metrics.clone());
    metrics
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
    (*metrics_for_textfield_shared(
        text,
        px,
        font_family,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    ))
    .clone()
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
    let index = ranges.partition_point(|range| range.start < byte);
    let range = ranges.get(index.checked_sub(1)?)?;
    (range.start < byte && byte < range.end).then_some(*range)
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

fn next_safe_boundary(measurement: &LineMeasurement, start: usize, limit: usize) -> usize {
    let offsets = &measurement.metrics.byte_offsets;
    let start = start.min(offsets.last().copied().unwrap_or(0));
    let limit = limit.min(offsets.last().copied().unwrap_or(0)).max(start);
    if start >= limit {
        return start;
    }
    let mut index = offsets.partition_point(|byte| *byte <= start);
    let mut next = offsets.get(index).copied().unwrap_or(limit);
    while next < limit {
        let Some(range) = containing_atomic_range(&measurement.atomic_ranges, next) else {
            break;
        };
        if range.end >= limit {
            return limit;
        }
        index = offsets.partition_point(|byte| *byte <= range.end);
        next = offsets.get(index).copied().unwrap_or(limit);
    }
    next.min(limit)
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
                let next = next_safe_boundary(measurement, probe, token_end);
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
                cut = next_safe_boundary(measurement, remaining_start, token_end);
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
        text: Arc::from(text),
        px_bits: px.to_bits(),
        max_width_bits: max_width.to_bits(),
        max_lines,
        family: font_family.map(Arc::from),
        soft_wrap,
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(Arc::from),
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
        return (*cached).clone();
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
    let result = Arc::new((lines, truncated));
    wrap_cache().lock().unwrap().put(key, result.clone());
    (result.0.clone(), result.1)
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
        return (*cached).clone();
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
        let result = Arc::new(result);
        wrap_ranges_cache().lock().unwrap().put(key, result.clone());
        return (result.0.clone(), result.1);
    }

    let mut output = Vec::new();
    let mut truncated = false;
    let hard_line_list = hard_lines(text);
    let hard_line_count = hard_line_list.len();
    let mut eng = engine().lock().unwrap();
    for line in hard_line_list {
        if max_lines.is_some_and(|limit| output.len() >= limit) {
            truncated = output.len() < hard_line_count;
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
    let result = Arc::new((output, truncated));
    wrap_ranges_cache().lock().unwrap().put(key, result.clone());
    (result.0.clone(), result.1)
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
        text: Arc::from(text),
        px_bits: px.to_bits(),
        max_width_bits: max_width.to_bits(),
        family: font_family.map(Arc::from),
        font_weight,
        font_style,
        letter_spacing_bits: letter_spacing.to_bits(),
        variation: font_variation_settings.map(Arc::from),
        generation: font_generation(),
    };
    if let Some(cached) = ellip_cache().lock().unwrap().get(&key).cloned() {
        return (*cached).clone();
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
        ellip_cache()
            .lock()
            .unwrap()
            .put(key, Arc::new(result.clone()));
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
        let result = Arc::new(String::new());
        ellip_cache().lock().unwrap().put(key, result);
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
    ellip_cache()
        .lock()
        .unwrap()
        .put(key, Arc::new(result.clone()));
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
        Mutex<Lru<(u32, Option<Arc<str>>, u16, u8, u32, Option<Arc<str>>, u64), f32>>,
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
        font_family.map(Arc::from),
        font_weight,
        font_style,
        letter_spacing.to_bits(),
        font_variation_settings.map(Arc::from),
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
