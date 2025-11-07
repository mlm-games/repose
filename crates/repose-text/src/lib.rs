use ahash::{AHashMap, AHasher};
use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, LayoutRunIter, Metrics, Shaping, SwashCache, SwashContent,
};
use once_cell::sync::OnceCell;
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::Mutex,
};
use unicode_segmentation::UnicodeSegmentation;

const WRAP_CACHE_CAP: usize = 1024;
const ELLIP_CACHE_CAP: usize = 2048;

struct Lru<K, V> {
    map: AHashMap<K, V>,
    order: VecDeque<K>,
    cap: usize,
}
impl<K: std::hash::Hash + Eq + Clone, V> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Self {
            map: AHashMap::new(),
            order: VecDeque::new(),
            cap,
        }
    }
    fn get(&mut self, k: &K) -> Option<&V> {
        if self.map.contains_key(k) {
            // move to back
            if let Some(pos) = self.order.iter().position(|x| x == k) {
                let key = self.order.remove(pos).unwrap();
                self.order.push_back(key);
            }
        }
        self.map.get(k)
    }
    fn put(&mut self, k: K, v: V) {
        if self.map.contains_key(&k) {
            self.map.insert(k.clone(), v);
            if let Some(pos) = self.order.iter().position(|x| x == &k) {
                let key = self.order.remove(pos).unwrap();
                self.order.push_back(key);
            }
            return;
        }
        if self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.order.push_back(k.clone());
        self.map.insert(k, v);
    }
}

static WRAP_LRU: OnceCell<Mutex<Lru<(u64, u32, u32, u16, bool), (Vec<String>, bool)>>> =
    OnceCell::new();
static ELLIP_LRU: OnceCell<Mutex<Lru<(u64, u32, u32), String>>> = OnceCell::new();

fn wrap_cache() -> &'static Mutex<Lru<(u64, u32, u32, u16, bool), (Vec<String>, bool)>> {
    WRAP_LRU.get_or_init(|| Mutex::new(Lru::new(WRAP_CACHE_CAP)))
}
fn ellip_cache() -> &'static Mutex<Lru<(u64, u32, u32), String>> {
    ELLIP_LRU.get_or_init(|| Mutex::new(Lru::new(ELLIP_CACHE_CAP)))
}

fn fast_hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = AHasher::default();
    s.hash(&mut h);
    h.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey(pub u64);

pub struct ShapedGlyph {
    pub key: GlyphKey,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
}

pub struct GlyphBitmap {
    pub key: GlyphKey,
    pub w: u32,
    pub h: u32,
    pub content: SwashContent,
    pub data: Vec<u8>, // Mask: A8; Color/Subpixel: RGBA8
}

struct Engine {
    fs: FontSystem,
    cache: SwashCache,
    // Map our compact atlas key -> full cosmic_text CacheKey
    key_map: HashMap<GlyphKey, CacheKey>,
}

impl Engine {
    fn get_image(&mut self, key: CacheKey) -> Option<cosmic_text::SwashImage> {
        // inside this method we may freely borrow both fields
        self.cache.get_image(&mut self.fs, key).clone()
    }
}

static ENGINE: OnceCell<Mutex<Engine>> = OnceCell::new();

fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| {
        let mut fs = FontSystem::new();

        let cache = SwashCache::new();

        static FALLBACK_TTF: &[u8] = include_bytes!("assets/OpenSans-Regular.ttf"); // GFonts, OFL licensed
        {
            // Register fallback font data into font DB
            let db = fs.db_mut();
            db.load_font_data(FALLBACK_TTF.to_vec());
            db.set_sans_serif_family("Open Sans".to_string());
        }
        Mutex::new(Engine {
            fs,
            cache,
            key_map: HashMap::new(),
        })
    })
}

// Utility: stable u64 key from a CacheKey using its Hash impl
fn key_from_cachekey(k: &CacheKey) -> GlyphKey {
    let mut h = AHasher::default();
    k.hash(&mut h);
    GlyphKey(h.finish())
}

// Shape a single-line string (no wrapping). Returns positioned glyphs relative to baseline y=0.
pub fn shape_line(text: &str, px: f32) -> Vec<ShapedGlyph> {
    let mut eng = engine().lock().unwrap();

    // Construct a temporary buffer each call; FontSystem and caches are retained globally
    let mut buf = Buffer::new(&mut eng.fs, Metrics::new(px, px * 1.3));
    {
        // Borrow with FS for ergonomic setters (no FS arg)
        let mut b = buf.borrow_with(&mut eng.fs);
        b.set_size(None, None);
        b.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        b.shape_until_scroll(true);
    }

    let mut out = Vec::new();
    for run in buf.layout_runs() {
        for g in run.glyphs {
            // Compute physical glyph: gives cache_key and integer pixel position
            let phys = g.physical((0.0, run.line_y), 1.0);
            let key = key_from_cachekey(&phys.cache_key);
            eng.key_map.insert(key, phys.cache_key);

            // Query raster cache to get placement for metrics
            let img_opt = eng.get_image(phys.cache_key);
            let (w, h, left, top) = if let Some(img) = img_opt.as_ref() {
                (
                    img.placement.width as f32,
                    img.placement.height as f32,
                    img.placement.left as f32,
                    img.placement.top as f32,
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };

            out.push(ShapedGlyph {
                key,
                x: g.x + g.x_offset, // visual x
                y: run.line_y,       // baseline y
                w,
                h,
                bearing_x: left,
                bearing_y: top,
                advance: g.w,
            });
        }
    }
    out
}

// Rasterize a glyph mask (A8) or color/subpixel (RGBA8) for a given shaped key.
// Returns owned pixels to avoid borrowing from the cache.
pub fn rasterize(key: GlyphKey, _px: f32) -> Option<GlyphBitmap> {
    let mut eng = engine().lock().unwrap();
    let &ck = eng.key_map.get(&key)?;

    let img = eng.get_image(ck).as_ref()?.clone();
    Some(GlyphBitmap {
        key,
        w: img.placement.width,
        h: img.placement.height,
        content: img.content,
        data: img.data, // already a Vec<u8>
    })
}

// Text metrics for TextField: positions per grapheme boundary and byte offsets.
pub struct TextMetrics {
    pub positions: Vec<f32>,      // cumulative advance per boundary (len == n+1)
    pub byte_offsets: Vec<usize>, // byte index per boundary (len == n+1)
}

// Compute caret mapping using shaping (no wrapping).
pub fn metrics_for_textfield(text: &str, px: f32) -> TextMetrics {
    let mut eng = engine().lock().unwrap();
    let mut buf = Buffer::new(&mut eng.fs, Metrics::new(px, px * 1.3));
    {
        let mut b = buf.borrow_with(&mut eng.fs);
        b.set_size(None, None);
        b.set_text(text, &Attrs::new(), Shaping::Advanced, None);
        b.shape_until_scroll(true);
    }

    let mut positions = vec![0.0f32];
    let mut byte_offsets = vec![0usize];
    let mut x = 0.0f32;

    for run in buf.layout_runs() {
        for g in run.glyphs {
            x = g.x + g.w; // right edge in LTR
            positions.push(x);
            byte_offsets.push(g.end);
        }
    }
    if *byte_offsets.last().unwrap_or(&0) != text.len() {
        positions.push(x);
        byte_offsets.push(text.len());
    }
    TextMetrics {
        positions,
        byte_offsets,
    }
}

/// Greedy wrap into lines that fit max_width. Prefers breaking at whitespace,
/// falls back to grapheme boundaries. If max_lines is Some and we truncate,
/// caller can choose to ellipsize the last visible line.
pub fn wrap_lines(
    text: &str,
    px: f32,
    max_width: f32,
    max_lines: Option<usize>,
    soft_wrap: bool,
) -> (Vec<String>, bool) {
    if text.is_empty() || max_width <= 0.0 {
        return (vec![String::new()], false);
    }
    if !soft_wrap {
        return (vec![text.to_string()], false);
    }

    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        (max_width * 100.0) as u32,
        max_lines.unwrap_or(usize::MAX) as u16,
        soft_wrap,
    );
    if let Some(h) = wrap_cache().lock().unwrap().get(&key).cloned() {
        return h;
    }

    // Shape once and reuse positions/byte mapping.
    let m = metrics_for_textfield(text, px);
    // Fast path: fits
    if let Some(&last) = m.positions.last() {
        if last <= max_width + 0.5 {
            return (vec![text.to_string()], false);
        }
    }

    // Helper: width of substring [start..end] in bytes
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

    let mut line_start = 0usize; // byte index
    let mut best_break = line_start;
    let mut last_w = 0.0;

    // Iterate word boundaries (keep whitespace tokens so they factor widths)
    for tok in text.split_word_bounds() {
        let tok_start = best_break;
        let tok_end = tok_start + tok.len();
        let w = width_of(line_start, tok_end);

        if w <= max_width + 0.5 {
            best_break = tok_end;
            last_w = w;
            continue;
        }

        // Need to break the line before tok_end.
        if best_break > line_start {
            // Break at last good boundary
            out.push(text[line_start..best_break].trim_end().to_string());
            line_start = best_break;
        } else {
            // Token itself too wide: force break inside token at grapheme boundaries
            let mut cut = tok_start;
            for g in tok.grapheme_indices(true) {
                let next = tok_start + g.0 + g.1.len();
                if width_of(line_start, next) <= max_width + 0.5 {
                    cut = next;
                } else {
                    break;
                }
            }
            if cut == line_start {
                // nothing fits; fall back to single grapheme
                if let Some((ofs, grapheme)) = tok.grapheme_indices(true).next() {
                    cut = tok_start + ofs + grapheme.len();
                }
            }
            out.push(text[line_start..cut].to_string());
            line_start = cut;
        }

        // Check max_lines
        if let Some(ml) = max_lines {
            if out.len() >= ml {
                truncated = true;
                // Stop; caller may ellipsize the last line
                line_start = line_start.min(text.len());
                break;
            }
        }

        // Reset best_break for new line
        best_break = line_start;
        last_w = 0.0;

        // Re-consider current token if not fully consumed
        if line_start < tok_end {
            // recompute width with the remaining token portion
            if width_of(line_start, tok_end) <= max_width + 0.5 {
                best_break = tok_end;
                last_w = width_of(line_start, best_break);
            } else {
                // will be handled in next iterations (or forced again)
            }
        }
    }

    // Push tail if allowed
    if line_start < text.len() && max_lines.map_or(true, |ml| out.len() < ml) {
        out.push(text[line_start..].trim_end().to_string());
    }

    let res = (out, truncated);

    wrap_cache().lock().unwrap().put(key, res.clone());
    res
}

/// Return a string truncated to fit max_width at the given px size, appending '…' if truncated.
pub fn ellipsize_line(text: &str, px: f32, max_width: f32) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    let key = (
        fast_hash(text),
        (px * 100.0) as u32,
        (max_width * 100.0) as u32,
    );
    if let Some(s) = ellip_cache().lock().unwrap().get(&key).cloned() {
        return s;
    }
    let m = metrics_for_textfield(text, px);
    if let Some(&last) = m.positions.last() {
        if last <= max_width + 0.5 {
            return text.to_string();
        }
    }
    let el = "…";
    let e_w = {
        let shaped = crate::shape_line(el, px);
        if let Some(g) = shaped.last() {
            g.x + g.advance
        } else {
            0.0
        }
    };
    if e_w >= max_width {
        return String::new();
    }
    // Find last grapheme index whose width + ellipsis fits
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
