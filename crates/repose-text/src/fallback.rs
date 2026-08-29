// Ported from Compose

use std::collections::{HashMap, HashSet};

use crate::fallback_data::{
    ENCODED_NOTO_FONT_SET_RANGES, ENCODED_NOTO_FONT_SETS, NOTO_FONTS, NotoFont,
};

// constants matching the Kotlin source
#[allow(dead_code)]
const FONT_FALLBACK_BASE_URL: &str = "https://fonts.gstatic.com/s/";
const PREFIX_DIGIT_0: u32 = 48;
const PREFIX_RADIX: u32 = 10;
const FONT_INDEX_DIGIT_0: u32 = 97; // 'a'
const FONT_INDEX_RADIX: u32 = 26;
const RANGE_SIZE_DIGIT_0: u32 = 97; // 'a'
const RANGE_SIZE_RADIX: u32 = 26;
const RANGE_VALUE_DIGIT_0: u32 = 65; // 'A'
const RANGE_VALUE_RADIX: u32 = 26;
const MAX_CODE_POINT: u32 = 0x10FFFF;

pub struct IndexedNotoFont {
    pub index: usize,
    pub font: &'static NotoFont,
    pub cover_count: usize,
    pub cover_components: Vec<usize>, // indices into components vec
}

pub struct FallbackFontComponent {
    pub fonts: Vec<usize>, // indices into indexed fonts arena
    pub cover_count: usize,
}

pub struct UnicodePropertyLookup {
    boundaries: Vec<u32>,
    // values[i] corresponds to range [boundaries[i-1]..boundaries[i])? Actually Kotlin logic:
    // boundaries holds end-exclusive start of next range, values is parallel.
    // lookup via binary search on boundaries (upper bound).
    values: Vec<FallbackFontComponent>,
}

impl UnicodePropertyLookup {
    pub fn lookup(&self, value: u32) -> &FallbackFontComponent {
        // Kotlin binary search: while true if start==end return values[start]
        // else mid, if value >= boundaries[mid] start=mid+1 else end=mid
        let mut start: usize = 0;
        let mut end: usize = self.boundaries.len();
        loop {
            if start == end {
                return &self.values[start];
            }
            let mid = start + (end - start) / 2;
            if value >= self.boundaries[mid] {
                start = mid + 1;
            } else {
                end = mid;
            }
        }
    }

    pub fn create() -> Self {
        // Decode font components from ENCODED_NOTO_FONT_SETS
        let property_enum_values = decode_font_components();

        // Decode boundaries / values from ENCODED_NOTO_FONT_SET_RANGES (packedData)
        let packed_data = ENCODED_NOTO_FONT_SET_RANGES;

        let mut boundaries: Vec<u32> = Vec::new();
        let mut values: Vec<FallbackFontComponent> = Vec::new();

        let mut start: u32 = 0;
        let mut prefix: u32 = 0;
        let mut size: u32 = 1;

        for ch in packed_data.chars() {
            let code = ch as u32;
            if (RANGE_VALUE_DIGIT_0..RANGE_VALUE_DIGIT_0 + RANGE_VALUE_RADIX).contains(&code) {
                let idx = (prefix * RANGE_VALUE_RADIX + (code - RANGE_VALUE_DIGIT_0)) as usize;
                // property_enum_values is Vec<FallbackFontComponent-template>; need clone
                let template = &property_enum_values[idx];
                // Clone fonts list for new component
                let comp = FallbackFontComponent {
                    fonts: template.fonts.clone(),
                    cover_count: 0,
                };
                start += size;
                boundaries.push(start);
                values.push(comp);
                prefix = 0;
                size = 1;
            } else if (RANGE_SIZE_DIGIT_0..RANGE_SIZE_DIGIT_0 + RANGE_SIZE_RADIX).contains(&code) {
                size = prefix * RANGE_SIZE_RADIX + (code - RANGE_SIZE_DIGIT_0) + 2;
                prefix = 0;
            } else if (PREFIX_DIGIT_0..PREFIX_DIGIT_0 + PREFIX_RADIX).contains(&code) {
                prefix = prefix * PREFIX_RADIX + (code - PREFIX_DIGIT_0);
            } else {
                panic!("Unexpected encoded range character: {}", ch);
            }
        }

        assert_eq!(
            start,
            MAX_CODE_POINT + 1,
            "Bad fallback map size: {}",
            start
        );

        Self { boundaries, values }
    }
}

fn decode_font_components() -> Vec<FallbackFontComponent> {
    ENCODED_NOTO_FONT_SETS
        .split(',')
        .map(|component_data| {
            let fonts = decode_font_set(component_data);
            FallbackFontComponent {
                fonts,
                cover_count: 0,
            }
        })
        .collect()
}

fn decode_font_set(data: &str) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::new();
    let mut previous_index: i32 = -1;
    let mut prefix: u32 = 0;
    for ch in data.chars() {
        let code = ch as u32;
        if (FONT_INDEX_DIGIT_0..FONT_INDEX_DIGIT_0 + FONT_INDEX_RADIX).contains(&code) {
            let delta = (prefix * FONT_INDEX_RADIX + (code - FONT_INDEX_DIGIT_0)) as i32;
            let index = previous_index + delta + 1;
            result.push(index as usize);
            previous_index = index;
            prefix = 0;
        } else if (PREFIX_DIGIT_0..PREFIX_DIGIT_0 + PREFIX_RADIX).contains(&code) {
            prefix = prefix * PREFIX_RADIX + (code - PREFIX_DIGIT_0);
        } else {
            panic!("Unexpected encoded font-set char: {}", ch);
        }
    }
    result
}

// NotoFontDownloader - port of getFontsToDownload logic
pub struct NotoFontDownloader {
    code_points_with_no_known_font: HashSet<u32>,
    lookup: UnicodePropertyLookup,
    // arena for IndexedNotoFont - created fresh per call in Kotlin via decoding, but we share lookup's fonts?
    // Kotlin creates IndexedNotoFont per decode (for each component). For efficiency we create per call.
}

impl NotoFontDownloader {
    pub fn new() -> Self {
        Self {
            code_points_with_no_known_font: HashSet::new(),
            lookup: UnicodePropertyLookup::create(),
        }
    }

    pub fn get_codepoints_with_no_known_font(&self) -> &HashSet<u32> {
        &self.code_points_with_no_known_font
    }

    /// Port of `getFontsToDownload` - returns list of NotoFonts to fetch.
    /// `language` is navigator.language like "ja", "zh-CN", etc.
    pub fn get_fonts_to_download(
        &mut self,
        codepoints: &HashSet<u32>,
        language: &str,
    ) -> Vec<&'static NotoFont> {
        if codepoints.is_empty() {
            return Vec::new();
        }

        // We need mutable coverCount tracking. Kotlin uses object fields.
        // We will create mutable copies of components and fonts.
        // Approach: clone lookup values into mutable vec, and create indexed fonts map.

        // First, determine which components are involved.
        // Build maps: codepoint -> component idx
        // But Kotlin's algorithm does per codepoint lookup and aggregates.
        // We'll replicate closely.

        // Create a working copy of values (components) with coverCount reset
        let mut components: Vec<FallbackFontComponent> = self
            .lookup
            .values
            .iter()
            .map(|c| FallbackFontComponent {
                fonts: c.fonts.clone(),
                cover_count: 0,
            })
            .collect();

        // Need mapping from font index -> IndexedNotoFont instance
        // Kotlin's IndexedNotoFont objects are shared across components (same object if same font index appears in multiple components).
        // We need to deduplicate.
        let mut font_index_to_obj: HashMap<usize, IndexedNotoFont> = HashMap::new();
        // Also build component index for each unique codepoint? Actually Kotlin aggregates by component identity:
        // For each codepoint, lookup returns a FallbackFontComponent reference (with its fonts list). But after decoding,
        // many codepoints share the same component object (via trie). In our port, each values[i] is a component.
        // So if two codepoints fall into same range (same boundary interval), they will lookup same values index.
        // Kotlin then does: if component.coverCount ==0 requiredComponents += component ; component.coverCount++
        // So deduplication is by component identity (index in values), not by fonts equality.
        // We must track component instances by their index in `components` vec.

        // To know which component each codepoint maps to, we can binary search boundaries manually (lookup) but need index.
        // Instead, we can get lookup index by performing same binary search returning idx.

        fn lookup_idx(boundaries: &[u32], value: u32) -> usize {
            let mut start = 0usize;
            let mut end = boundaries.len();
            loop {
                if start == end {
                    return start;
                }
                let mid = start + (end - start) / 2;
                if value >= boundaries[mid] {
                    start = mid + 1;
                } else {
                    end = mid;
                }
            }
        }

        let mut missing: Vec<u32> = Vec::new();
        let mut required_component_indices: Vec<usize> = Vec::new();

        for &cp in codepoints {
            if self.code_points_with_no_known_font.contains(&cp) || cp > MAX_CODE_POINT {
                continue;
            }
            let idx = lookup_idx(&self.lookup.boundaries, cp);
            let comp = &mut components[idx];
            if comp.fonts.is_empty() {
                missing.push(cp);
            } else {
                if comp.cover_count == 0 {
                    required_component_indices.push(idx);
                }
                comp.cover_count += 1;
            }
        }

        if !missing.is_empty() {
            self.code_points_with_no_known_font.extend(missing);
        }

        if required_component_indices.is_empty() {
            return Vec::new();
        }

        // Build font arena and candidate list
        // Ensure font objects exist for all fonts referenced in required components
        for &comp_idx in &required_component_indices {
            for &font_idx in &components[comp_idx].fonts.clone() {
                font_index_to_obj
                    .entry(font_idx)
                    .or_insert_with(|| IndexedNotoFont {
                        index: font_idx,
                        font: &NOTO_FONTS[font_idx],
                        cover_count: 0,
                        cover_components: Vec::new(),
                    });
            }
        }

        // Populate candidateFonts: for each required component, for each font in it, update coverCount
        let mut candidate_font_indices: HashSet<usize> = HashSet::new();

        for &comp_idx in &required_component_indices {
            let comp_cover = components[comp_idx].cover_count;
            let fonts_clone = components[comp_idx].fonts.clone();
            for font_idx in fonts_clone {
                let font_obj = font_index_to_obj.get_mut(&font_idx).unwrap();
                if font_obj.cover_count == 0 {
                    candidate_font_indices.insert(font_idx);
                }
                font_obj.cover_count += comp_cover;
                font_obj.cover_components.push(comp_idx);
            }
        }

        // Convert candidate set to vec for iteration
        let mut candidate_vec: Vec<usize> = candidate_font_indices.into_iter().collect();

        let mut selected: Vec<&'static NotoFont> = Vec::new();

        // Greedy selection loop
        while !candidate_vec.is_empty() {
            // select best font among candidates
            let best_idx = select_font(&candidate_vec, &font_index_to_obj, language);
            let best_font = font_index_to_obj.get(&best_idx).unwrap();
            selected.push(best_font.font);

            // Remove its coverage
            let covered_components: Vec<usize> = best_font.cover_components.clone();
            for comp_idx in covered_components {
                let comp_cover = components[comp_idx].cover_count;
                let fonts_in_comp = components[comp_idx].fonts.clone();
                for f_idx in fonts_in_comp {
                    if let Some(fobj) = font_index_to_obj.get_mut(&f_idx) {
                        fobj.cover_count = fobj.cover_count.saturating_sub(comp_cover);
                        // remove component from its cover list
                        fobj.cover_components.retain(|&c| c != comp_idx);
                    }
                }
                components[comp_idx].cover_count = 0;
            }

            // Remove fonts with zero cover
            candidate_vec.retain(|fid| {
                font_index_to_obj
                    .get(fid)
                    .map(|f| f.cover_count != 0)
                    .unwrap_or(false)
            });
        }

        // distinctBy index already guaranteed by set
        selected
    }
}

fn is_cjk_font(font: &NotoFont) -> bool {
    is_noto_sans_sc(font)
        || is_noto_sans_tc(font)
        || is_noto_sans_hk(font)
        || is_noto_sans_jp(font)
        || is_noto_sans_kr(font)
}
fn is_noto_sans_sc(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans SC")
}
fn is_noto_sans_tc(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans TC")
}
fn is_noto_sans_hk(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans HK")
}
fn is_noto_sans_jp(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans JP")
}
fn is_noto_sans_kr(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans KR")
}
fn is_noto_color_emoji(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Color Emoji")
}
fn is_noto_sans_symbols(f: &NotoFont) -> bool {
    f.name.starts_with("Noto Sans Symbols")
}

fn select_font(
    candidates: &[usize],
    arena: &HashMap<usize, IndexedNotoFont>,
    language: &str,
) -> usize {
    // Find max coverCount
    let mut max_covered = -1i32;
    let mut best_fonts: Vec<usize> = Vec::new();
    let mut best: Option<usize> = None;
    for &fid in candidates {
        let f = &arena[&fid];
        let cc = f.cover_count as i32;
        if cc > max_covered {
            best_fonts.clear();
            best_fonts.push(fid);
            best = Some(fid);
            max_covered = cc;
        } else if cc == max_covered {
            best_fonts.push(fid);
            if best.map(|b| fid < b).unwrap_or(true) {
                best = Some(fid);
            }
        }
    }

    // Language tie-break
    if best_fonts.len() > 1 {
        // check if all best are cjk
        let all_cjk = best_fonts.iter().all(|&fid| is_cjk_font(arena[&fid].font));
        if all_cjk {
            if let Some(idx) = select_best_for_language(&best_fonts, arena, language) {
                return idx;
            }
            if let Some(idx) = select_best_for_language(candidates, arena, language) {
                return idx;
            }
        } else {
            // emoji/symbols preference
            if let Some(&fid) = best_fonts
                .iter()
                .find(|&&fid| is_noto_color_emoji(arena[&fid].font))
            {
                return fid;
            }
            if let Some(&fid) = best_fonts
                .iter()
                .find(|&&fid| is_noto_sans_symbols(arena[&fid].font))
            {
                return fid;
            }
            if let Some(&fid) = best_fonts
                .iter()
                .find(|&&fid| is_noto_sans_sc(arena[&fid].font))
            {
                return fid;
            }
        }
    }

    best.expect("No fallback font selected")
}

fn select_best_for_language(
    candidates: &[usize],
    arena: &HashMap<usize, IndexedNotoFont>,
    language: &str,
) -> Option<usize> {
    match language {
        "zh-Hans" | "zh-CN" | "zh-SG" | "zh-MY" => candidates
            .iter()
            .find(|&&fid| is_noto_sans_sc(arena[&fid].font))
            .copied(),
        "zh-Hant" | "zh-TW" | "zh-MO" => candidates
            .iter()
            .find(|&&fid| is_noto_sans_tc(arena[&fid].font))
            .copied(),
        "zh-HK" => candidates
            .iter()
            .find(|&&fid| is_noto_sans_hk(arena[&fid].font))
            .copied(),
        "ja" => candidates
            .iter()
            .find(|&&fid| is_noto_sans_jp(arena[&fid].font))
            .copied(),
        "ko" => candidates
            .iter()
            .find(|&&fid| is_noto_sans_kr(arena[&fid].font))
            .copied(),
        _ => None,
    }
}

// WASM download + registry (mirrors WebFallbackFontDownloader)

#[cfg(target_arch = "wasm32")]
pub mod wasm_fallback {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, Response};

    const BATCH_WINDOW_MS: u32 = 60;
    const MAX_BATCH_SIZE: usize = 10;

    thread_local! {
        static GLOBAL: Rc<RefCell<FallbackManager>> = Rc::new(RefCell::new(FallbackManager::new()));
    }

    struct FallbackManager {
        downloader: NotoFontDownloader,
        pending: HashSet<u32>,
        // simple channel via Vec
        queued: Vec<HashSet<u32>>,
        is_running: bool,
        error_count: u32,
    }

    impl FallbackManager {
        fn new() -> Self {
            Self {
                downloader: NotoFontDownloader::new(),
                pending: HashSet::new(),
                queued: Vec::new(),
                is_running: false,
                error_count: 0,
            }
        }
    }

    pub fn submit_unresolved(codepoints: Vec<u32>) {
        if codepoints.is_empty() {
            return;
        }
        let set: HashSet<u32> = codepoints.into_iter().collect();
        GLOBAL.with(|g| {
            let mut mgr = g.borrow_mut();
            mgr.queued.push(set);
            if !mgr.is_running {
                mgr.is_running = true;
                let g_clone = g.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    run_loop(g_clone).await;
                });
            }
        });
    }

    async fn run_loop(global: Rc<RefCell<FallbackManager>>) {
        loop {
            // await batch
            let batch = {
                // wait for at least one queued item
                loop {
                    let has = global.borrow().queued.len() > 0;
                    if has {
                        break;
                    }
                    gloo_timers_approx_delay(16).await;
                }
                // collect batch
                let mut batch_set = HashSet::new();
                // take first
                {
                    let mut mgr = global.borrow_mut();
                    if let Some(first) = mgr.queued.pop() {
                        batch_set.extend(first);
                    }
                }
                // collect up to 9 more within 60ms
                let mut collected = 1;
                let start = js_sys::Date::now();
                while collected < MAX_BATCH_SIZE {
                    let elapsed = js_sys::Date::now() - start;
                    if elapsed >= BATCH_WINDOW_MS as f64 {
                        break;
                    }
                    let maybe = { global.borrow_mut().queued.pop() };
                    if let Some(s) = maybe {
                        batch_set.extend(s);
                        collected += 1;
                    } else {
                        // sleep a bit
                        gloo_timers_approx_delay(10).await;
                        if global.borrow().queued.is_empty() {
                            // no more, break after window
                            if js_sys::Date::now() - start >= BATCH_WINDOW_MS as f64 {
                                break;
                            }
                        }
                    }
                }
                batch_set
            };

            if batch.is_empty() {
                continue;
            }

            // attempt download
            let fonts_to_download: Vec<&'static NotoFont> = {
                let mut mgr = global.borrow_mut();
                // language detection
                let lang = web_sys::window()
                    .and_then(|w| w.navigator().language())
                    .unwrap_or_else(|| "en".to_string());
                let res = mgr.downloader.get_fonts_to_download(&batch, &lang);
                res
            };

            if fonts_to_download.is_empty() {
                // nothing to do, drain?
                continue;
            }

            let mut successes: Vec<Vec<u8>> = Vec::new();
            let mut any_success = false;
            let mut all_failed = true;

            for font in &fonts_to_download {
                let url = format!("{}{}", FONT_FALLBACK_BASE_URL, font.url);
                match fetch_bytes(&url).await {
                    Ok(bytes) => {
                        successes.push(bytes);
                        any_success = true;
                        all_failed = false;
                    }
                    Err(e) => {
                        log::warn!("Failed to download fallback font [{}]: {:?}", url, e);
                    }
                }
            }

            if all_failed && any_success == false && !fonts_to_download.is_empty() {
                // error handling with backoff
                let backoff = {
                    let mut mgr = global.borrow_mut();
                    let pause = mgr.error_count * 5;
                    mgr.error_count += 1;
                    pause
                };
                log::warn!("Fallback download failed, retry in {}s", backoff);
                // delay then re-queue batch
                gloo_timers_approx_delay(backoff * 1000).await;
                global.borrow_mut().queued.push(batch);
                continue;
            }

            // success: reset error count, register fonts
            {
                global.borrow_mut().error_count = 0;
            }

            for bytes in successes {
                crate::register_font_data(&bytes);
            }

            if any_success {
                // clear unresolved? In Kotlin they clear registry and notify listeners.
                // We need to invalidate caches and request frame.
                crate::clear_caches_for_fallback();
                // notify via custom event? For now request animation frame via repose-core if available
                // We can try to use web_sys window requestAnimationFrame to trigger redraw?
                // Simpler: dispatch custom event that platform can listen? Instead, just bump frame counter.
                crate::bump_frame_for_fallback();
                // drain excess channel? In Kotlin drainChannel discards pending batches beyond 10.
                // Our queued already handled; we clear extra if needed.
                // Limit handled by MAX_BATCH_SIZE, extra remains for next loop.
            }
        }
    }

    async fn gloo_timers_approx_delay(ms: u32) -> () {
        // use js_sys Promise with setTimeout without extra crate
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let window = web_sys::window().unwrap();
            let _ =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        });
        let _ = JsFuture::from(promise).await;
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, JsValue> {
        let request = Request::new_with_str(url)?;
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value.dyn_into().unwrap();
        if !resp.ok() {
            return Err(JsValue::from_str(&format!(
                "fetch failed status {}",
                resp.status()
            )));
        }
        let buffer = JsFuture::from(resp.array_buffer()?).await?;
        let arr = js_sys::Uint8Array::new(&buffer);
        Ok(arr.to_vec())
    }

    // Public API for init
    pub fn ensure_fallback_initialized() {
        // Ensure GLOBAL exists
        GLOBAL.with(|_| {});
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod wasm_fallback {
    pub fn submit_unresolved(_codepoints: Vec<u32>) {}
    pub fn ensure_fallback_initialized() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn empty_returns_empty() {
        let mut d = NotoFontDownloader::new();
        let res = d.get_fonts_to_download(&HashSet::new(), "en");
        assert!(res.is_empty());
        assert!(d.get_codepoints_with_no_known_font().is_empty());
    }

    #[test]
    fn above_max_ignored() {
        let mut d = NotoFontDownloader::new();
        let mut set = HashSet::new();
        set.insert(0x110000);
        let res = d.get_fonts_to_download(&set, "en");
        assert!(res.is_empty());
        assert!(d.get_codepoints_with_no_known_font().is_empty());
    }

    #[test]
    fn pua_remembered() {
        let mut d = NotoFontDownloader::new();
        let mut set = HashSet::new();
        set.insert(0xE000);
        let first = d.get_fonts_to_download(&set, "en");
        assert!(first.is_empty());
        assert!(d.get_codepoints_with_no_known_font().contains(&0xE000));
        let second = d.get_fonts_to_download(&set, "en");
        assert!(second.is_empty());
    }

    #[test]
    fn arabic_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x0639, 0x0641].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty(), "Arabic should resolve");
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans Arabic")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn emoji_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x1F600, 0x1F389].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Color Emoji")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cjk_zh_cn() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x5B57].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "zh-CN");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans SC")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn japanese_hiragana() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x3042].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "ja");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans JP")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn devanagari() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x0905, 0x0915].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty(), "Devanagari should resolve to Noto Sans*");
        // Compose test says Noto Sans (base) for 0905,0915
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hebrew_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x05E9, 0x05D0].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans Hebrew")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn thai_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x0E01, 0x0E2A].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans Thai")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn bengali_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x0985, 0x0995].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty());
        assert!(
            fonts
                .iter()
                .all(|f| f.name.starts_with("Noto Sans Bengali")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn box_drawing_resolves() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x2500, 0x2502].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "en");
        assert!(!fonts.is_empty());
        // Box drawing may be in HK or SC depending on generated data version; just check Noto Sans*
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn korean_ko() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0xACA8, 0xACAF, 0xACF0].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "ko");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans KR")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cjk_zh_hant() {
        let mut d = NotoFontDownloader::new();
        let set: HashSet<u32> = [0x5B57].into_iter().collect();
        let fonts = d.get_fonts_to_download(&set, "zh-TW");
        assert!(!fonts.is_empty());
        assert!(
            fonts.iter().all(|f| f.name.starts_with("Noto Sans TC")),
            "got {:?}",
            fonts.iter().map(|f| f.name).collect::<Vec<_>>()
        );
    }
}
