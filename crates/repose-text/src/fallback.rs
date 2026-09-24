// Ported from Compose

use std::collections::{HashMap, HashSet};

use crate::fallback_data::{
    ENCODED_NOTO_FONT_SET_RANGES, ENCODED_NOTO_FONT_SETS, NOTO_FONTS, NotoFont,
};

// constants matching the Kotlin source
const PREFIX_DIGIT_0: u32 = 48;
const PREFIX_RADIX: u32 = 10;
const FONT_INDEX_DIGIT_0: u32 = 97; // 'a'
const FONT_INDEX_RADIX: u32 = 26;
const RANGE_SIZE_DIGIT_0: u32 = 97; // 'a'
const RANGE_SIZE_RADIX: u32 = 26;
const RANGE_VALUE_DIGIT_0: u32 = 65; // 'A'
const RANGE_VALUE_RADIX: u32 = 26;
const MAX_CODE_POINT: u32 = 0x10FFFF;

#[cfg(target_arch = "wasm32")]
fn direct_font_url(font: &NotoFont) -> String {
    font.url.to_owned()
}

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
    pub fn lookup(&self, value: u32) -> Option<&FallbackFontComponent> {
        if value > MAX_CODE_POINT {
            return None;
        }
        let mut start: usize = 0;
        let mut end: usize = self.boundaries.len();
        loop {
            if start == end {
                return self.values.get(start);
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

impl Default for NotoFontDownloader {
    fn default() -> Self {
        Self::new()
    }
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

        fn lookup_idx(boundaries: &[u32], value: u32) -> Option<usize> {
            if value > MAX_CODE_POINT {
                return None;
            }
            let mut start = 0usize;
            let mut end = boundaries.len();
            loop {
                if start == end {
                    return Some(start);
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
        let mut codepoints: Vec<u32> = codepoints.iter().copied().collect();
        codepoints.sort_unstable();

        for cp in codepoints {
            if self.code_points_with_no_known_font.contains(&cp) || cp > MAX_CODE_POINT {
                continue;
            }
            let Some(idx) = lookup_idx(&self.lookup.boundaries, cp) else {
                continue;
            };
            let Some(comp) = components.get_mut(idx) else {
                continue;
            };
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
        candidate_vec.sort_unstable();

        let mut selected: Vec<&'static NotoFont> = Vec::new();

        // Greedy selection loop
        while !candidate_vec.is_empty() {
            // select best font among candidates
            let best_idx = select_font(&candidate_vec, &font_index_to_obj, language);
            let best_font = font_index_to_obj.get(&best_idx).unwrap();
            selected.push(best_font.font);

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

    const MAX_BATCH_SIZE: usize = 10;

    thread_local! {
        static GLOBAL: Rc<RefCell<WebFallbackFontDownloader>> = Rc::new(RefCell::new(WebFallbackFontDownloader::new()));
        static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
        static LISTENER_KEEP: RefCell<Option<std::sync::Arc<dyn crate::unresolved::UnresolvedListener>>> = const { RefCell::new(None) };
    }

    struct WebFallbackFontDownloader {
        downloader: NotoFontDownloader,
        queued: Vec<HashSet<u32>>,
        is_running: bool,
        error_count: u32,
    }

    impl WebFallbackFontDownloader {
        fn new() -> Self {
            Self {
                downloader: NotoFontDownloader::new(),
                queued: Vec::new(),
                is_running: false,
                error_count: 0,
            }
        }

        fn submit(&mut self, codepoints: HashSet<u32>) {
            if codepoints.is_empty() {
                return;
            }
            self.queued.push(codepoints);
        }
    }

    fn start_worker(global: Rc<RefCell<WebFallbackFontDownloader>>) {
        let should_start = {
            let mut state = global.borrow_mut();
            if state.is_running {
                false
            } else {
                state.is_running = true;
                true
            }
        };
        if should_start {
            wasm_bindgen_futures::spawn_local(async move {
                run_worker(global).await;
            });
        }
    }

    fn enqueue(codepoints: HashSet<u32>) {
        let global = GLOBAL.with(|global| {
            let mut state = global.borrow_mut();
            if let Some(queued) = state.queued.last_mut() {
                queued.extend(codepoints);
            } else {
                state.submit(codepoints);
            }
            global.clone()
        });
        start_worker(global);
    }

    pub fn submit_unresolved(codepoints: Vec<u32>) {
        if codepoints.is_empty() {
            return;
        }
        let set: HashSet<u32> = codepoints.into_iter().collect();
        let installed = INSTALLED.with(|installed| *installed.borrow());
        crate::unresolved::web_unresolved_registry()
            .add_unresolved_vec(set.iter().copied().collect());
        if !installed {
            enqueue(set);
        }
    }

    pub fn install_fallback_font_downloader() {
        let already = INSTALLED.with(|v| {
            let mut b = v.borrow_mut();
            if *b {
                true
            } else {
                *b = true;
                false
            }
        });
        if already {
            return;
        }

        // Create listener that forwards registry -> downloader channel (mirrors UnresolvedSymbolsRegistry.Listener)
        struct RegistryListener;
        impl crate::unresolved::UnresolvedListener for RegistryListener {
            fn on_unresolved_codepoints(&self, codepoints: &HashSet<u32>) {
                enqueue(codepoints.clone());
            }
            fn on_new_font_installed(&self) {
                // In compose ParagraphLayouter invalidates paragraph on new font
                // Here global caches already cleared via register_font_data
            }
        }

        let listener: std::sync::Arc<dyn crate::unresolved::UnresolvedListener> =
            std::sync::Arc::new(RegistryListener);
        crate::unresolved::web_unresolved_registry().add_listener(listener.clone());
        // so Weak in registry doesn't die (was a bug prev.: listener dropped after fn)
        LISTENER_KEEP.with(|c| *c.borrow_mut() = Some(listener));
        let pending = crate::unresolved::web_unresolved_registry().snapshot();
        if !pending.is_empty() {
            enqueue(pending);
        }
    }

    async fn run_worker(global: Rc<RefCell<WebFallbackFontDownloader>>) {
        loop {
            let batch = {
                let mut state = global.borrow_mut();
                if state.queued.is_empty() {
                    state.is_running = false;
                    return;
                }
                let mut batch = state.queued.remove(0);
                let mut count = 1;
                while count < MAX_BATCH_SIZE && !state.queued.is_empty() {
                    batch.extend(state.queued.remove(0));
                    count += 1;
                }
                batch
            };

            if batch.is_empty() {
                continue;
            }

            let fonts_to_download: Vec<&'static NotoFont> = {
                let mut state = global.borrow_mut();
                let language = web_sys::window()
                    .and_then(|window| window.navigator().language())
                    .unwrap_or_else(|| "en".to_string());
                state.downloader.get_fonts_to_download(&batch, &language)
            };

            if fonts_to_download.is_empty() {
                continue;
            }

            let mut successes: Vec<Vec<u8>> = Vec::new();
            let mut failed = false;
            let mut seen_urls: HashSet<String> = HashSet::new();
            for font in &fonts_to_download {
                let url = direct_font_url(font);
                if !seen_urls.insert(url.clone()) {
                    continue;
                }
                match fetch_bytes(&url).await {
                    Ok(bytes) => successes.push(bytes),
                    Err(_) => failed = true,
                }
            }

            let mut any_success = false;
            for bytes in successes {
                if crate::register_font_data_if_usable(&bytes) {
                    any_success = true;
                } else {
                    failed = true;
                }
            }

            if failed || !any_success {
                let backoff = {
                    let mut state = global.borrow_mut();
                    let pause = state.error_count.saturating_mul(5).min(60);
                    state.error_count = state.error_count.saturating_add(1);
                    pause
                };
                if backoff > 0 {
                    gloo_timers_approx_delay(backoff.saturating_mul(1000)).await;
                }
                global.borrow_mut().queued.push(batch);
                continue;
            }

            global.borrow_mut().error_count = 0;
            crate::unresolved::web_unresolved_registry().on_new_font_installed();
        }
    }

    async fn gloo_timers_approx_delay(ms: u32) -> () {
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let Some(window) = web_sys::window() else {
                let _ = resolve.call0(&JsValue::NULL);
                return;
            };
            let _ =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32);
        });
        let _ = JsFuture::from(promise).await;
    }

    async fn fetch_bytes(url: &str) -> Result<Vec<u8>, JsValue> {
        let opts = web_sys::RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(web_sys::RequestMode::Cors);
        opts.set_cache(web_sys::RequestCache::Default);
        let request = Request::new_with_str_and_init(url, &opts)?;
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value
            .dyn_into()
            .map_err(|_| JsValue::from_str("fetch did not return a Response"))?;
        if !resp.ok() {
            return Err(JsValue::from_str(&format!(
                "fetch failed status {}",
                resp.status()
            )));
        }
        let buffer = JsFuture::from(resp.array_buffer()?).await?;
        let arr = js_sys::Uint8Array::new(&buffer);
        let bytes = arr.to_vec();
        Ok(bytes)
    }

    pub fn ensure_fallback_initialized() {
        install_fallback_font_downloader();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod wasm_fallback {
    pub fn submit_unresolved(_codepoints: Vec<u32>) {}
    pub fn ensure_fallback_initialized() {}
    pub fn install_fallback_font_downloader() {}
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
