use accesskit::{
    Action, ActionHandler, ActionRequest, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate,
};
use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::runtime::SemNode;
use crate::semantics::LiveRegion as CoreLiveRegion;
use crate::semantics::Role as CoreRole;

pub const WINDOW_ID: NodeId = NodeId(1);

pub struct ReposeActionHandler {
    pub pending_actions: Arc<Mutex<Vec<ActionRequest>>>,
}

impl Default for ReposeActionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ReposeActionHandler {
    pub fn new() -> Self {
        Self {
            pending_actions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ActionHandler for ReposeActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let mut q = self.pending_actions.lock().unwrap();
        q.push(request);
    }
}

#[derive(Default)]
pub struct A11yTree {
    prev_hash: FxHashMap<u64, u64>,
    prev_root_hash: u64,
    prev_focus: Option<u64>,
    initialized: bool,
}

impl A11yTree {
    pub fn initial_tree() -> TreeUpdate {
        let root = Node::new(Role::Window);
        TreeUpdate {
            nodes: vec![(WINDOW_ID, root)],
            tree: Some(TreeInfo::new(WINDOW_ID)),
            focus: WINDOW_ID,
            tree_id: TreeId::ROOT,
        }
    }

    pub fn update(
        &mut self,
        sems: &[SemNode],
        scale: f64,
        focused_id: Option<u64>,
    ) -> Option<TreeUpdate> {
        let (root_children, children_map) = build_children(sems);

        let mut changed_nodes: Vec<(NodeId, Node)> = Vec::new();

        let focus = focused_id.map(NodeId).unwrap_or(WINDOW_ID);
        let focus_changed = self.prev_focus != focused_id;
        self.prev_focus = focused_id;

        // root hash includes ordered root children + scale (bounds math depends on scale)
        let root_hash = {
            let mut h = rustc_hash::FxHasher::default();
            (scale.to_bits()).hash(&mut h);
            root_children.len().hash(&mut h);
            for &id in &root_children {
                id.hash(&mut h);
            }
            h.finish()
        };

        let root_changed = !self.initialized || self.prev_root_hash != root_hash;
        self.prev_root_hash = root_hash;

        if root_changed {
            let mut root = Node::new(Role::Window);
            root.set_children(
                root_children
                    .iter()
                    .copied()
                    .map(NodeId)
                    .collect::<Vec<_>>(),
            );
            changed_nodes.push((WINDOW_ID, root));
        }

        let mut new_hashes: FxHashMap<u64, u64> = FxHashMap::default();

        for sem in sems {
            if sem.hidden {
                continue;
            }
            let kids = children_map
                .get(&sem.id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let node_hash = hash_sem_node(sem, kids, scale);
            new_hashes.insert(sem.id, node_hash);

            let prev = self.prev_hash.get(&sem.id).copied();
            let needs_update = !self.initialized || prev != Some(node_hash);

            if needs_update {
                let node = build_accesskit_node(sem, kids, scale);
                changed_nodes.push((NodeId(sem.id), node));
            }
        }

        self.prev_hash = new_hashes;
        self.initialized = true;

        if changed_nodes.is_empty() && !focus_changed {
            return None;
        }

        Some(TreeUpdate {
            nodes: changed_nodes,
            tree: None,
            focus,
            tree_id: TreeId::ROOT,
        })
    }
}

fn build_children(sems: &[SemNode]) -> (Vec<u64>, FxHashMap<u64, Vec<u64>>) {
    let mut roots: Vec<u64> = Vec::new();
    let mut map: FxHashMap<u64, Vec<u64>> = FxHashMap::default();
    let mut hidden: FxHashMap<u64, bool> = FxHashMap::default();
    let mut parent_of: FxHashMap<u64, Option<u64>> = FxHashMap::default();
    for s in sems {
        hidden.insert(s.id, s.hidden);
        parent_of.insert(s.id, s.parent);
    }
    for s in sems {
        if s.hidden {
            continue;
        }
        let mut ancestor = s.parent;
        while let Some(pid) = ancestor {
            if !hidden.get(&pid).copied().unwrap_or(false) {
                break;
            }
            ancestor = parent_of.get(&pid).copied().flatten();
        }
        match ancestor {
            Some(pid) => map.entry(pid).or_default().push(s.id),
            None => roots.push(s.id),
        }
    }
    sort_traversal(sems, &mut roots);
    for kids in map.values_mut() {
        sort_traversal(sems, kids);
    }

    (roots, map)
}

fn sort_traversal(sems: &[SemNode], ids: &mut Vec<u64>) {
    if ids
        .iter()
        .any(|id| sems.iter().any(|n| n.id == *id && n.traversal_index.is_some()))
    {
        ids.sort_by(|a, b| {
            let ta = sems
                .iter()
                .find(|n| n.id == *a)
                .and_then(|n| n.traversal_index)
                .unwrap_or(f32::MAX);
            let tb = sems
                .iter()
                .find(|n| n.id == *b)
                .and_then(|n| n.traversal_index)
                .unwrap_or(f32::MAX);
            ta.partial_cmp(&tb).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

fn build_accesskit_node(sem: &SemNode, children: &[u64], scale: f64) -> Node {
    let mut node = Node::new(map_role(sem.role));

    let r = sem.rect;
    node.set_bounds(Rect {
        x0: r.x as f64 / scale,
        y0: r.y as f64 / scale,
        x1: (r.x + r.w) as f64 / scale,
        y1: (r.y + r.h) as f64 / scale,
    });

    if let Some(label) = &sem.label
        && !label.is_empty()
    {
        node.set_label(label.clone());
    }

    if matches!(sem.role, CoreRole::Dialog) {
        node.set_modal();
    }

    if sem.heading {
        node.set_role(Role::Heading);
    }

    if let Some(region) = sem.live_region {
        node.set_live(match region {
            CoreLiveRegion::Polite => accesskit::Live::Polite,
            CoreLiveRegion::Assertive => accesskit::Live::Assertive,
        });
        node.set_live_atomic();
    }

    if let Some(description) = &sem.state_description {
        node.set_state_description(description.clone());
    } else if let Some(error) = &sem.error {
        node.set_state_description(error.clone());
    }

    if sem.error.is_some() {
        node.set_invalid(accesskit::Invalid::True);
    }

    if let Some(expanded) = sem.expanded {
        node.set_expanded(expanded);
    }

    if let Some(rows) = sem.collection_rows {
        node.set_row_count(rows);
    }
    if let Some(columns) = sem.collection_columns {
        node.set_column_count(columns);
    }
    if let Some(row) = sem.row_index {
        node.set_row_index(row);
    }
    if let Some(column) = sem.column_index {
        node.set_column_index(column);
    }

    if let Some(progress) = sem.progress {
        node.set_min_numeric_value(progress.min as f64);
        node.set_max_numeric_value(progress.max as f64);
        node.set_numeric_value(progress.current as f64);
        if let Some(step) = progress.step {
            node.set_numeric_value_step(step as f64);
        }
    }

    if !children.is_empty() {
        node.set_children(children.iter().copied().map(NodeId).collect::<Vec<_>>());
    }

    if let Some(checked) = sem.checked {
        node.set_toggled(checked.into());
    }
    if let Some(selected) = sem.selected {
        node.set_selected(selected);
    }
    if let Some(v) = &sem.value {
        node.set_value(v.clone());
    }

    if sem.enabled {
        match sem.role {
            CoreRole::Button
            | CoreRole::Checkbox
            | CoreRole::Switch
            | CoreRole::RadioButton
            | CoreRole::Tab => {
                node.add_action(Action::Click);
            }
            CoreRole::TextField | CoreRole::Slider => {
                node.add_action(Action::Focus);
            }
            _ => {}
        }
    }

    if sem.scrollable {
        node.add_action(Action::ScrollUp);
        node.add_action(Action::ScrollDown);
        node.add_action(Action::ScrollLeft);
        node.add_action(Action::ScrollRight);
        node.add_action(Action::ScrollIntoView);
    }
    if matches!(sem.role, CoreRole::Slider) {
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
        node.add_action(Action::SetValue);
    }
    if matches!(sem.role, CoreRole::TextField) {
        node.add_action(Action::ReplaceSelectedText);
        node.add_action(Action::SetTextSelection);
    }
    if sem.can_expand {
        node.add_action(Action::Expand);
    }
    if sem.can_collapse {
        node.add_action(Action::Collapse);
    }

    node
}

fn map_role(role: CoreRole) -> Role {
    match role {
        CoreRole::Text => Role::Label,
        CoreRole::Button => Role::Button,
        CoreRole::TextField => Role::TextInput,
        CoreRole::Container => Role::GenericContainer,
        CoreRole::Checkbox => Role::CheckBox,
        CoreRole::RadioButton => Role::RadioButton,
        CoreRole::Switch => Role::Switch,
        CoreRole::Slider => Role::Slider,
        CoreRole::ProgressBar => Role::ProgressIndicator,
        CoreRole::Tab => Role::Tab,
        CoreRole::Dialog => Role::Dialog,
        CoreRole::Image => Role::Image,
        CoreRole::DropdownList => Role::ComboBox,
        CoreRole::ValuePicker => Role::SpinButton,
        CoreRole::Carousel => Role::List,
    }
}

fn hash_sem_node(sem: &SemNode, children: &[u64], scale: f64) -> u64 {
    let mut h = rustc_hash::FxHasher::default();

    (scale.to_bits()).hash(&mut h);

    sem.id.hash(&mut h);
    sem.parent.hash(&mut h);
    std::mem::discriminant(&sem.role).hash(&mut h);

    let q = |v: f32| (v * 8.0) as i32;
    q(sem.rect.x).hash(&mut h);
    q(sem.rect.y).hash(&mut h);
    q(sem.rect.w).hash(&mut h);
    q(sem.rect.h).hash(&mut h);

    sem.focused.hash(&mut h);
    sem.enabled.hash(&mut h);
    sem.selectable_group.hash(&mut h);
    sem.checked.hash(&mut h);
    sem.selected.hash(&mut h);
    sem.heading.hash(&mut h);
    sem.hidden.hash(&mut h);
    sem.is_traversal_group.hash(&mut h);
    sem.scrollable.hash(&mut h);
    sem.can_expand.hash(&mut h);
    sem.can_collapse.hash(&mut h);
    sem.can_dismiss.hash(&mut h);

    if let Some(lbl) = &sem.label {
        lbl.hash(&mut h);
    }
    if let Some(v) = &sem.value {
        v.hash(&mut h);
    }
    if let Some(d) = &sem.state_description {
        d.hash(&mut h);
    }
    if let Some(e) = &sem.error {
        e.hash(&mut h);
    }
    if let Some(t) = sem.traversal_index {
        t.to_bits().hash(&mut h);
    }
    if let Some(lr) = sem.live_region {
        std::mem::discriminant(&lr).hash(&mut h);
    }
    sem.expanded.hash(&mut h);
    sem.collection_rows.hash(&mut h);
    sem.collection_columns.hash(&mut h);
    sem.row_index.hash(&mut h);
    sem.column_index.hash(&mut h);
    if let Some(p) = sem.progress {
        p.current.to_bits().hash(&mut h);
        p.min.to_bits().hash(&mut h);
        p.max.to_bits().hash(&mut h);
        p.step.map(|s| s.to_bits()).hash(&mut h);
    }

    children.len().hash(&mut h);
    for &c in children {
        c.hash(&mut h);
    }

    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::SemNode;

    fn node(id: u64, parent: Option<u64>, role: CoreRole) -> SemNode {
        SemNode {
            id,
            parent,
            role,
            ..Default::default()
        }
    }

    #[test]
    fn test_new_roles_map_to_accesskit() {
        assert_eq!(map_role(CoreRole::Image), Role::Image);
        assert_eq!(map_role(CoreRole::DropdownList), Role::ComboBox);
        assert_eq!(map_role(CoreRole::ValuePicker), Role::SpinButton);
        assert_eq!(map_role(CoreRole::Carousel), Role::List);
    }

    #[test]
    fn test_heading_live_and_progress_extras() {
        let mut sem = node(1, None, CoreRole::Text);
        sem.heading = true;
        sem.live_region = Some(CoreLiveRegion::Assertive);
        let built = build_accesskit_node(&sem, &[], 1.0);
        assert_eq!(built.role(), Role::Heading);
        assert_eq!(built.live(), Some(accesskit::Live::Assertive));

        let mut slider = node(2, None, CoreRole::Slider);
        slider.progress = Some(crate::semantics::ProgressRangeInfo {
            current: 0.5,
            min: 0.0,
            max: 1.0,
            step: Some(0.1),
        });
        let built = build_accesskit_node(&slider, &[], 1.0);
        assert_eq!(built.numeric_value(), Some(0.5));
        assert!(built.supports_action(Action::SetValue));
        assert!(built.supports_action(Action::Increment));
    }

    #[test]
    fn test_hidden_prunes_and_reparents_with_traversal_sort() {
        let mut parent = node(1, None, CoreRole::Container);
        parent.hidden = true;
        let mut a = node(2, Some(1), CoreRole::Button);
        a.traversal_index = Some(2.0);
        let mut b = node(3, Some(1), CoreRole::Button);
        b.traversal_index = Some(1.0);
        let sems = vec![parent, a, b];
        let (roots, map) = build_children(&sems);
        assert_eq!(roots, vec![3, 2]);
        assert!(map.is_empty());

        let mut tree = A11yTree::default();
        let update = tree.update(&sems, 1.0, None).expect("initial update");
        assert!(!update.nodes.iter().any(|(id, _)| *id == NodeId(1)));
    }

    #[test]
    fn test_scrollable_and_expand_actions() {
        let mut sem = node(1, None, CoreRole::Container);
        sem.scrollable = true;
        sem.can_expand = true;
        let built = build_accesskit_node(&sem, &[], 1.0);
        assert!(built.supports_action(Action::ScrollDown));
        assert!(built.supports_action(Action::ScrollIntoView));
        assert!(built.supports_action(Action::Expand));

        let mut field = node(2, None, CoreRole::TextField);
        field.error = Some("Required".into());
        let built = build_accesskit_node(&field, &[], 1.0);
        assert!(built.supports_action(Action::ReplaceSelectedText));
        assert_eq!(built.invalid(), Some(accesskit::Invalid::True));
    }
}
