//! Scene strategies: which back-stack entries share the screen.
//!
//! A [`SceneStrategy`] receives the whole back stack and returns the scenes to
//! compose. [`SinglePaneSceneStrategy`] shows the top entry only;
//! [`ListDetailSceneStrategy`] keeps the entry below the top beside it on wide
//! windows; [`DialogSceneStrategy`] lifts matching entries into an overlay.

use std::rc::Rc;

use repose_core::*;
use repose_ui::{
    Box as VBox, BoxWithConstraints, Column, Row, ViewExt, ZStack, anim::animate_f32_from,
};

use crate::{
    EntryRenderer, EntryScope, NavBackStack, NavKey, NavTransition, Navigator, TransitionDir,
};

/// What a scene is doing on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneRole {
    /// Fills the space left by the other scenes.
    Primary,
    /// A side-by-side pane, laid out after the primary scene.
    Secondary,
    /// Painted over the whole area with a scrim.
    Dialog,
}

/// One screen the strategy wants composed, made of one or more back-stack
/// entries. `entries` is ordered back-most first, so the last id is the entry
/// whose content is current.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub entries: Vec<u64>,
    pub role: SceneRole,
}

impl Scene {
    pub fn new(entries: impl Into<Vec<u64>>, role: SceneRole) -> Self {
        Self {
            entries: entries.into(),
            role,
        }
    }

    pub fn top(&self) -> Option<u64> {
        self.entries.last().copied()
    }

    fn animation_key(&self) -> String {
        let ids: Vec<String> = self.entries.iter().map(|id| id.to_string()).collect();
        format!("{:?}:{}", self.role, ids.join("."))
    }
}

/// One back-stack entry as the strategy sees it.
#[derive(Clone, Debug)]
pub struct NavEntryInfo<K: NavKey> {
    pub id: u64,
    pub key: K,
    pub is_current: bool,
}

/// Chooses the scenes to compose for a back stack.
pub trait SceneStrategy<K: NavKey> {
    /// `entries` is ordered root-first; the last element is the top entry.
    /// Implementations return at least one scene for a non-empty stack.
    fn scenes(&self, entries: &[NavEntryInfo<K>]) -> Vec<Scene>;
}

/// Shows only the top entry.
pub struct SinglePaneSceneStrategy;

impl<K: NavKey> SceneStrategy<K> for SinglePaneSceneStrategy {
    fn scenes(&self, entries: &[NavEntryInfo<K>]) -> Vec<Scene> {
        entries
            .last()
            .map(|entry| vec![Scene::new([entry.id], SceneRole::Primary)])
            .unwrap_or_default()
    }
}

/// How [`ListDetailSceneStrategy`] decides between one and two panes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneMode {
    /// Follow the current window size class: two panes from medium width up.
    #[default]
    Adaptive,
    TwoPane,
    SinglePane,
}

/// Shows the entry below the top beside the top one when the window is wide
/// enough. Narrow windows collapse to the top entry.
#[derive(Default)]
pub struct ListDetailSceneStrategy {
    pub mode: PaneMode,
}

impl ListDetailSceneStrategy {
    pub fn adaptive() -> Self {
        Self {
            mode: PaneMode::Adaptive,
        }
    }

    pub fn always() -> Self {
        Self {
            mode: PaneMode::TwoPane,
        }
    }

    pub fn single() -> Self {
        Self {
            mode: PaneMode::SinglePane,
        }
    }
}

impl<K: NavKey> SceneStrategy<K> for ListDetailSceneStrategy {
    fn scenes(&self, entries: &[NavEntryInfo<K>]) -> Vec<Scene> {
        let Some(top) = entries.last() else {
            return Vec::new();
        };
        let two_pane = match self.mode {
            PaneMode::Adaptive => locals::window_size_class().is_at_least_medium_width(),
            PaneMode::TwoPane => true,
            PaneMode::SinglePane => false,
        };
        if !two_pane || entries.len() == 1 {
            return vec![Scene::new([top.id], SceneRole::Primary)];
        }
        let list = &entries[entries.len() - 2];
        vec![
            Scene::new([list.id], SceneRole::Primary),
            Scene::new([top.id], SceneRole::Secondary),
        ]
    }
}

/// Wraps a base strategy and lifts matching entries into dialog scenes. The
/// base strategy still decides the area scenes, so the screen underneath keeps
/// its layout while a dialog covers it.
pub struct DialogSceneStrategy<K: NavKey> {
    base: Rc<dyn SceneStrategy<K>>,
    is_dialog: Rc<dyn Fn(&K) -> bool>,
}

impl<K: NavKey> DialogSceneStrategy<K> {
    pub fn new(base: Rc<dyn SceneStrategy<K>>, is_dialog: impl Fn(&K) -> bool + 'static) -> Self {
        Self {
            base,
            is_dialog: Rc::new(is_dialog),
        }
    }

    pub fn single_pane(is_dialog: impl Fn(&K) -> bool + 'static) -> Self {
        Self::new(Rc::new(SinglePaneSceneStrategy), is_dialog)
    }

    pub fn list_detail(is_dialog: impl Fn(&K) -> bool + 'static) -> Self {
        Self::new(Rc::new(ListDetailSceneStrategy::adaptive()), is_dialog)
    }
}

impl<K: NavKey> SceneStrategy<K> for DialogSceneStrategy<K> {
    fn scenes(&self, entries: &[NavEntryInfo<K>]) -> Vec<Scene> {
        let mut area: Vec<NavEntryInfo<K>> = Vec::new();
        let mut dialogs: Vec<Scene> = Vec::new();
        for entry in entries {
            if (self.is_dialog)(&entry.key) {
                dialogs.push(Scene::new([entry.id], SceneRole::Dialog));
            } else {
                area.push(entry.clone());
            }
        }
        let mut scenes = self.base.scenes(&area);
        scenes.extend(dialogs);
        scenes
    }
}

/// Compose a back stack through a [`SceneStrategy`].
///
/// Area scenes lay out in a row; every dialog overlays the area with a scrim.
/// Only a single primary scene gets the push/pop slide and predictive back.
pub fn NavDisplayWithStrategy<K: NavKey>(
    stack: Rc<NavBackStack<K>>,
    strategy: Rc<dyn SceneStrategy<K>>,
    make_view: EntryRenderer<K>,
    on_back: Option<Rc<dyn Fn()>>,
    transition: NavTransition,
) -> View {
    let _version = stack.version.get();
    let snapshot = stack.snapshot();
    let current_id = stack.current_id();
    let infos: Vec<NavEntryInfo<K>> = snapshot
        .iter()
        .map(|(id, key, _, _)| NavEntryInfo {
            id: *id,
            key: key.clone(),
            is_current: Some(*id) == current_id,
        })
        .collect();
    let scenes = visible_scenes(strategy.scenes(&infos));
    if scenes.is_empty() {
        return VBox(Modifier::new());
    }

    let framed = if scenes.len() == 1 && scenes[0].role == SceneRole::Primary {
        compose_primary_transition(&stack, &scenes[0], transition, &make_view)
    } else {
        compose_scenes(&stack, &scenes, transition, &make_view)
    };
    maybe_intercept_back(framed, on_back)
}

/// A dialog covers everything below it, so only the top-most dialog can be
/// seen. Dropping the hidden ones keeps scrims from stacking.
fn visible_scenes(mut scenes: Vec<Scene>) -> Vec<Scene> {
    let first_dialog = scenes
        .iter()
        .position(|scene| scene.role == SceneRole::Dialog);
    let Some(first_dialog) = first_dialog else {
        return scenes;
    };
    let dialogs = scenes.split_off(first_dialog);
    if let Some(last) = dialogs.last() {
        scenes.push(last.clone());
    }
    scenes
}

fn compose_primary_transition<K: NavKey>(
    stack: &Rc<NavBackStack<K>>,
    scene: &Scene,
    transition: NavTransition,
    make_view: &EntryRenderer<K>,
) -> View {
    let dir = stack.last_dir();
    let gesture_pop = dir == TransitionDir::Pop && stack.gesture_pop();
    let gesture = repose_core::back::gesture();
    if gesture.progress > 0.0 {
        let current = compose_scene(stack, scene, make_view);
        return with_back_gesture(stack, &gesture, current, make_view);
    }
    let composed = compose_scene(stack, scene, make_view);
    if dir == TransitionDir::None || gesture_pop {
        return composed;
    }

    let (initial, target) = if dir == TransitionDir::Push {
        (0.0, 1.0)
    } else {
        (1.0, 0.0)
    };
    let t = animate_f32_from(
        format!(
            "nav3:{}:{}:{}",
            stack.stack_id,
            stack.transition_id(),
            scene.animation_key()
        ),
        initial,
        target,
        transition.spec,
    );
    let slide = if dir == TransitionDir::Push { 1.0 - t } else { t };
    let dx = slide * transition.slide_px * if dir == TransitionDir::Push { 1.0 } else { -1.0 };
    let alpha = if transition.fade {
        0.75 + 0.25 * (1.0 - slide)
    } else {
        1.0
    };
    Column(Modifier::new().fill_max_size()).child(
        VBox(
            Modifier::new()
                .fill_max_size()
                .translate(dx, 0.0)
                .alpha(alpha),
        )
        .child(composed),
    )
}

fn compose_scenes<K: NavKey>(
    stack: &Rc<NavBackStack<K>>,
    scenes: &[Scene],
    transition: NavTransition,
    make_view: &EntryRenderer<K>,
) -> View {
    let area: Vec<&Scene> = scenes
        .iter()
        .filter(|scene| scene.role != SceneRole::Dialog)
        .collect();
    let dialogs: Vec<&Scene> = scenes
        .iter()
        .filter(|scene| scene.role == SceneRole::Dialog)
        .collect();

    let area_view = if area.len() > 1 {
        let mut row = Row(Modifier::new().fill_max_size());
        for scene in &area {
            row = row.child(
                VBox(Modifier::new().fill_max_size().flex_grow(1.0))
                    .child(compose_scene(stack, scene, make_view)),
            );
        }
        row
    } else if let Some(scene) = area.first() {
        compose_scene(stack, scene, make_view)
    } else {
        VBox(Modifier::new().fill_max_size())
    };

    if dialogs.is_empty() {
        return area_view;
    }

    let mut layered = ZStack(Modifier::new().fill_max_size().overflow(Overflow::Clip)).child(area_view);
    for scene in dialogs {
        let id = scene.top().unwrap_or(0);
        let t = animate_f32_from(
            format!("nav3:dialog:{}:{}", stack.stack_id, id),
            0.0,
            1.0,
            transition.spec,
        );
        let scrim = VBox(
            Modifier::new()
                .fill_max_size()
                .background(Color::from_rgba(0, 0, 0, (96.0 * t) as u8))
                .on_pointer_down(|_| {}),
        );
        layered = layered.child(scrim).child(
            VBox(
                Modifier::new()
                    .fill_max_size()
                    .alpha(t)
                    .content_alignment(Alignment::Center)
                    .translate(0.0, (1.0 - t) * 24.0),
            )
            .child(compose_scene(stack, scene, make_view)),
        );
    }
    layered
}

fn compose_scene<K: NavKey>(
    stack: &Rc<NavBackStack<K>>,
    scene: &Scene,
    make_view: &EntryRenderer<K>,
) -> View {
    let Some(id) = scene.top() else {
        return VBox(Modifier::new());
    };
    let Some((key, saved, scope)) = stack.entry(id) else {
        return VBox(Modifier::new());
    };
    let entry = EntryScope::new(
        id,
        key,
        saved,
        scope,
        Navigator {
            stack: (**stack).clone(),
        },
    );
    entry.run(|| make_view(&entry))
}

fn with_back_gesture<K: NavKey>(
    stack: &Rc<NavBackStack<K>>,
    gesture: &repose_core::back::BackGesture,
    current: View,
    make_view: &EntryRenderer<K>,
) -> View {
    let progress = gesture.progress;
    if progress <= 0.0 || stack.size() < 2 {
        return current;
    }
    let from = stack.size() - 2;
    let Some((previous_id, previous_key, previous_saved, previous_scope)) =
        stack.snapshot().get(from).cloned()
    else {
        return current;
    };
    let under = EntryScope::new(
        previous_id,
        previous_key,
        previous_saved,
        previous_scope,
        Navigator {
            stack: (**stack).clone(),
        },
    );
    let progress = progress.clamp(0.0, 1.0);
    let edge_sign = match gesture.edge {
        repose_core::back::SwipeEdge::Left => 1.0,
        repose_core::back::SwipeEdge::Right => -1.0,
    };
    let make_view = make_view.clone();
    BoxWithConstraints(Modifier::new().fill_max_size(), move |constraints| {
        let width_dp = if constraints.has_bounded_width() && constraints.max_width.0 > 0.0 {
            constraints.max_width.0
        } else {
            repose_core::locals::get_window_container_width()
        };
        let page_width = width_dp * repose_core::locals::density().scale;
        let travel = edge_sign * page_width * progress;
        let under_travel = -edge_sign * page_width * (1.0 - progress);
        let under_view = under.run(|| make_view(&under));
        let current_view = current.clone();
        ZStack(Modifier::new().fill_max_size().overflow(Overflow::Clip))
            .child(
                VBox(
                    Modifier::new()
                        .fill_max_size()
                        .background(theme().background)
                        .translate(under_travel, 0.0)
                        .alpha(progress),
                )
                .child(under_view),
            )
            .child(
                VBox(
                    Modifier::new()
                        .fill_max_size()
                        .background(theme().background)
                        .translate(travel, 0.0)
                        .alpha(1.0 - progress),
                )
                .child(current_view),
            )
    })
}

fn maybe_intercept_back(v: View, on_back: Option<Rc<dyn Fn()>>) -> View {
    let Some(on_back) = on_back else { return v };
    VBox(
        Modifier::new()
            .semantics(Semantics::new(Role::Container))
            .on_preview_key_event(move |ke: KeyEvent| {
                if ke.event_type != KeyEventType::Down || ke.is_repeat {
                    return false;
                }
                let action = repose_core::shortcuts::resolve_action(
                    repose_core::shortcuts::KeyChord::new(ke.key.clone(), ke.modifiers),
                );
                if ke.key == Key::Escape
                    || matches!(action, Some(repose_core::shortcuts::Action::Back))
                {
                    on_back();
                    true
                } else {
                    false
                }
            }),
    )
    .child(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: u64, key: &str, is_current: bool) -> NavEntryInfo<String> {
        NavEntryInfo {
            id,
            key: key.to_string(),
            is_current,
        }
    }

    #[test]
    fn single_pane_keeps_only_the_top_entry() {
        let entries = [info(1, "home", false), info(2, "detail", true)];
        let scenes = SinglePaneSceneStrategy.scenes(&entries);
        assert_eq!(scenes, vec![Scene::new([2], SceneRole::Primary)]);
    }

    #[test]
    fn list_detail_pairs_the_two_top_entries_when_wide() {
        let entries = [info(1, "home", false), info(2, "detail", true)];
        let scenes = ListDetailSceneStrategy::always().scenes(&entries);
        assert_eq!(
            scenes,
            vec![
                Scene::new([1], SceneRole::Primary),
                Scene::new([2], SceneRole::Secondary),
            ]
        );
    }

    #[test]
    fn list_detail_collapses_when_narrow_or_rooted() {
        let entries = [info(1, "home", false), info(2, "detail", true)];
        assert_eq!(
            ListDetailSceneStrategy::single().scenes(&entries),
            vec![Scene::new([2], SceneRole::Primary)]
        );
        assert_eq!(
            ListDetailSceneStrategy::always().scenes(&entries[..1]),
            vec![Scene::new([1], SceneRole::Primary)]
        );
    }

    #[test]
    fn dialog_strategy_lifts_dialog_entries_over_the_area() {
        let strategy = DialogSceneStrategy::single_pane(|key: &String| key.starts_with("dialog"));
        let entries = [
            info(1, "home", false),
            info(2, "detail", false),
            info(3, "dialog:settings", true),
        ];
        let scenes = strategy.scenes(&entries);
        assert_eq!(
            scenes,
            vec![
                Scene::new([2], SceneRole::Primary),
                Scene::new([3], SceneRole::Dialog),
            ]
        );
    }

    #[test]
    fn covered_dialogs_are_not_composed() {
        let scenes = visible_scenes(vec![
            Scene::new([1], SceneRole::Primary),
            Scene::new([2], SceneRole::Dialog),
            Scene::new([3], SceneRole::Dialog),
        ]);
        assert_eq!(
            scenes,
            vec![
                Scene::new([1], SceneRole::Primary),
                Scene::new([3], SceneRole::Dialog),
            ]
        );
    }
}
