//! SubcomposeLayout and BoxWithConstraints.
//!
//! These layouts compose their children during the *reconcile* pass using the
//! current available size, so the inner content can adapt to the parent's
//! constraints.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use repose_core::{BoxWithConstraintsScope, Modifier, SubcomposeScope, View, ViewKind};

/// Hash any `Hash` value into a `u64` suitable for use as a
/// [`Modifier::key`](repose_core::Modifier::key).
///
/// This is what the `*_with_key` helpers use internally. It is exposed for
/// callers who want to set the key on a `Modifier` directly.
pub fn subcompose_hash_key<K: Hash>(key: &K) -> u64 {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

/// A layout whose `content` closure is invoked with the current available
/// size (in dp) and returns one or more `(slot_id, view)` pairs.
///
/// The single-slot form takes a closure that returns a single `View`; that
/// view is implicitly assigned slot id `0`. The multi-slot form takes a
/// closure returning a `Vec<(u64, View)>` and is exposed by
/// [`subcompose_layout_with_slots`] for callers that need multiple slots.
///
/// `content` runs during reconcile. The first frame the closure is called
/// and its result is cached; subsequent frames reuse the cached result as
/// long as the available scope (and the `SubcomposeLayout`'s modifier) are
/// unchanged.
///
/// If `content` captures state (such as a `Signal`) whose changes should
/// re-trigger the closure, use [`subcompose_with_key`] (or
/// [`box_with_constraints_with_key`]) so the cache is invalidated when that
/// state changes.
pub fn SubcomposeLayout<F>(modifier: Modifier, content: F) -> View
where
    F: Fn(SubcomposeScope) -> View + 'static,
{
    let wrapped: Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>> =
        Arc::new(move |scope| vec![(0, content(*scope))]);
    View {
        id: 0,
        kind: ViewKind::SubcomposeLayout { content: wrapped },
        modifier,
        children: Vec::new(),
        semantics: None,
    }
}

/// Multi-slot variant of [`SubcomposeLayout`]. The `content` closure receives
/// the current scope and returns a list of `(slot_id, view)` pairs. Slot ids
/// are stable across frames: removing or reordering slots preserves the
/// underlying tree nodes.
pub fn subcompose_layout_with_slots<F>(modifier: Modifier, content: F) -> View
where
    F: Fn(SubcomposeScope) -> Vec<(u64, View)> + 'static,
{
    let wrapped: Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>> =
        Arc::new(move |scope| content(*scope));
    View {
        id: 0,
        kind: ViewKind::SubcomposeLayout { content: wrapped },
        modifier,
        children: Vec::new(),
        semantics: None,
    }
}

/// A [`SubcomposeLayout`] specialized for the "show different content based on
/// the available width/height" use case.
///
/// The supplied `content` receives a [`BoxWithConstraintsScope`] containing the
/// current constraints (in dp) and returns the `View` to render. The resulting
/// view fills the available space.
pub fn BoxWithConstraints<F>(modifier: Modifier, content: F) -> View
where
    F: Fn(BoxWithConstraintsScope) -> View + 'static,
{
    SubcomposeLayout(modifier, move |scope| {
        content(BoxWithConstraintsScope {
            min_width: scope.min_width,
            max_width: scope.max_width,
            min_height: scope.min_height,
            max_height: scope.max_height,
        })
    })
}

/// Build a [`SubcomposeLayout`] that re-invokes its `content` closure whenever
/// the hashed value of `key` changes.
///
/// Use this when the closure captures state that should re-trigger
/// subcomposition. Typical pattern: read the signal *outside* the closure and
/// pass the value here so the cache key changes when the signal changes.
///
/// ```ignore
/// let count = signal.get();
/// subcompose_with_key(count, modifier, move |scope| {
///     let count = signal.get();  // inner read observes the same value
///     Text(format!("count = {count}"))
/// });
/// ```
pub fn subcompose_with_key<K, F>(key: K, modifier: Modifier, content: F) -> View
where
    K: Hash,
    F: Fn(SubcomposeScope) -> View + 'static,
{
    let hashed = subcompose_hash_key(&key);
    SubcomposeLayout(modifier.key(hashed), content)
}

/// Keyed variant of [`BoxWithConstraints`]. Re-invokes `content` whenever the
/// hashed value of `key` changes.
pub fn box_with_constraints_with_key<K, F>(key: K, modifier: Modifier, content: F) -> View
where
    K: Hash,
    F: Fn(BoxWithConstraintsScope) -> View + 'static,
{
    subcompose_with_key(key, modifier, move |scope| {
        content(BoxWithConstraintsScope {
            min_width: scope.min_width,
            max_width: scope.max_width,
            min_height: scope.min_height,
            max_height: scope.max_height,
        })
    })
}

/// Multi-slot keyed variant of [`subcompose_layout_with_slots`]. The `key`'s
/// hashed value is attached to the resulting `SubcomposeLayout` so the cache
/// is invalidated whenever the key changes.
pub fn subcompose_with_key_slots<K, F>(key: K, modifier: Modifier, content: F) -> View
where
    K: Hash,
    F: Fn(SubcomposeScope) -> Vec<(u64, View)> + 'static,
{
    let hashed = subcompose_hash_key(&key);
    subcompose_layout_with_slots(modifier.key(hashed), content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutEngine;
    use crate::{Column, Interactions, ViewExt};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn text_view(text: &str) -> View {
        use repose_core::{Color, TextOverflow, ViewKind};
        View {
            id: 0,
            kind: ViewKind::Text {
                text: text.to_string(),
                color: Color::WHITE,
                font_size: 14.0,
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
            },
            modifier: Modifier::default(),
            children: vec![],
            semantics: None,
        }
    }

    fn make_root(view: View) -> View {
        Column(Modifier::new()).child(view)
    }

    #[test]
    fn subcompose_hash_key_is_deterministic_and_distinguishes_values() {
        assert_eq!(subcompose_hash_key(&"hello"), subcompose_hash_key(&"hello"));
        assert_ne!(subcompose_hash_key(&"hello"), subcompose_hash_key(&"world"));
        assert_eq!(
            subcompose_hash_key(&(1u32, 2u32)),
            subcompose_hash_key(&(1u32, 2u32))
        );
        assert_ne!(
            subcompose_hash_key(&(1u32, 2u32)),
            subcompose_hash_key(&(1u32, 3u32))
        );
    }

    #[test]
    fn subcompose_with_key_runs_closure_once_until_key_changes() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = calls.clone();

        // Both roots share the same Arc<AtomicUsize> so the second closure's
        // increments are visible to the assertion below.
        let sub = subcompose_with_key(1u64, Modifier::new(), move |_scope| {
            calls_c.fetch_add(1, Ordering::SeqCst);
            text_view("k=1")
        });
        let root_v1 = make_root(sub);

        let calls2 = calls.clone();
        let sub = subcompose_with_key(2u64, Modifier::new(), move |_scope| {
            calls2.fetch_add(1, Ordering::SeqCst);
            text_view("k=2")
        });
        let root_v2 = make_root(sub);

        let mut engine = LayoutEngine::new();

        // Frame with key=1: closure runs once.
        let _ = engine.layout_frame(
            &root_v1,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Repeat the same key=1 frame: closure stays cached.
        let _ = engine.layout_frame(
            &root_v1,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Now switch to key=2 (a different subcompose node) and verify the
        // new closure runs.
        let _ = engine.layout_frame(
            &root_v2,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn box_with_constraints_with_key_forwards_scope() {
        use crate::Box as RBox;
        let sub = box_with_constraints_with_key(42u64, Modifier::new(), |scope| {
            assert!(scope.max_width > 0.0);
            RBox(Modifier::new())
        });
        // Smoke check: builds a valid View with the SubcomposeLayout kind.
        match sub.kind {
            ViewKind::SubcomposeLayout { .. } => {}
            _ => panic!("expected SubcomposeLayout"),
        }
    }

    #[test]
    fn subcompose_with_key_slots_runs_closure_once_until_key_changes() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = calls.clone();

        let sub = subcompose_with_key_slots(1u64, Modifier::new(), move |_scope| {
            calls_c.fetch_add(1, Ordering::SeqCst);
            vec![(0, text_view("k=1")), (1, text_view("k=1b"))]
        });
        let root_v1 = make_root(sub);

        let calls2 = calls.clone();
        let sub2 = subcompose_with_key_slots(2u64, Modifier::new(), move |_scope| {
            calls2.fetch_add(1, Ordering::SeqCst);
            vec![(0, text_view("k=2"))]
        });
        let root_v2 = make_root(sub2);

        let mut engine = LayoutEngine::new();

        let _ = engine.layout_frame(
            &root_v1,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Same key: cache hit.
        let _ = engine.layout_frame(
            &root_v1,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // New key: closure runs.
        let _ = engine.layout_frame(
            &root_v2,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
