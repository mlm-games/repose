use ahash::AHasher;
use cosmic_text::{
    Attrs, Buffer, CacheKey, FontSystem, LayoutRunIter, Metrics, Shaping, SwashCache, SwashContent,
};
use once_cell::sync::OnceCell;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Mutex,
};

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
        let fs = FontSystem::new();
        let cache = SwashCache::new();
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
