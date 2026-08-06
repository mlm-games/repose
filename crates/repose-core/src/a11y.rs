use accesskit::{
    Action, ActionHandler, ActionRequest, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate,
};
use rustc_hash::FxHashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::runtime::SemNode;
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
            tree: Some(Tree::new(WINDOW_ID)),
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

    for s in sems {
        if let Some(p) = s.parent {
            map.entry(p).or_default().push(s.id);
        } else {
            roots.push(s.id);
        }
    }

    (roots, map)
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

    if !children.is_empty() {
        node.set_children(children.iter().copied().map(NodeId).collect::<Vec<_>>());
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

    if let Some(lbl) = &sem.label {
        lbl.hash(&mut h);
    }

    children.len().hash(&mut h);
    for &c in children {
        c.hash(&mut h);
    }

    h.finish()
}
