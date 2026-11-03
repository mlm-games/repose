//! Persistent modifier nodes: state attached to a modifier that survives
//! recomposition.
//!
//! A [`ModifierNodeElement`] describes a node; [`Modifier::node`] attaches it.
//! During reconciliation the tree keeps the previous node when the element type
//! and key match, and calls [`ModifierNodeElement::update`] on it instead of
//! building a new one. Node state therefore survives the modifier being rebuilt
//! every frame, which is what gestures, timers, and scroll physics need.
//!
//! [`Modifier::node`]: crate::modifier::Modifier::node

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A live modifier node.
///
/// Implementors keep their own state behind interior mutability. Lifecycle
/// callbacks run once: `on_attach` the first time the node is painted,
/// `on_detach` when the last handle referencing it is dropped.
pub trait ModifierNode: Any {
    fn on_attach(&self) {}
    fn on_detach(&self) {}
    /// Upcast hook used to downcast to the concrete node type.
    fn as_any(&self) -> &dyn Any;
}

/// A description of one modifier node.
///
/// `create` builds the node when there is nothing to reuse. `update` copies the
/// element's current configuration into an existing node without replacing its
/// state, which is what makes a node persistent.
pub trait ModifierNodeElement: Any {
    fn create(&self) -> Rc<dyn ModifierNode>;
    fn update(&self, node: &Rc<dyn ModifierNode>);
    /// Extra identity beyond the element type. Two elements of the same type
    /// with different keys never share a node.
    fn key(&self) -> Option<u64> {
        None
    }
}

/// One element and its node, stored on a [`Modifier`](crate::Modifier).
///
/// The node is created on first use, so a modifier that never reaches a tree
/// pays nothing for its nodes.
pub struct ModifierNodeHandle {
    element: Rc<dyn ModifierNodeElement>,
    node: RefCell<Option<Rc<dyn ModifierNode>>>,
    attached: Rc<Cell<bool>>,
}

impl ModifierNodeHandle {
    pub fn new(element: Rc<dyn ModifierNodeElement>) -> Self {
        Self {
            element,
            node: RefCell::new(None),
            attached: Rc::new(Cell::new(false)),
        }
    }

    /// The live node, created on first access.
    pub fn node(&self) -> Rc<dyn ModifierNode> {
        if let Some(node) = self.node.borrow().as_ref() {
            return node.clone();
        }
        let node = self.element.create();
        *self.node.borrow_mut() = Some(node.clone());
        node
    }

    pub fn element(&self) -> &Rc<dyn ModifierNodeElement> {
        &self.element
    }

    /// Whether this handle would reuse `previous`'s node.
    pub fn same_identity(&self, previous: &Self) -> bool {
        self.element.type_id() == previous.element.type_id()
            && self.element.key() == previous.element.key()
    }

    /// Reuse `previous`'s node and push this element's configuration into it.
    pub(crate) fn adopt(&mut self, previous: &Self) {
        if !self.same_identity(previous) {
            return;
        }
        let node = previous.node();
        *self.node.borrow_mut() = Some(node.clone());
        self.attached = previous.attached.clone();
        self.element.update(&node);
    }

    pub(crate) fn ensure_attached(&self) {
        if !self.attached.replace(true) {
            self.node().on_attach();
        }
    }

    #[doc(hidden)]
    pub fn is_attached(&self) -> bool {
        self.attached.get()
    }
}

impl Clone for ModifierNodeHandle {
    fn clone(&self) -> Self {
        Self {
            element: self.element.clone(),
            node: RefCell::new(self.node.borrow().clone()),
            attached: self.attached.clone(),
        }
    }
}

impl Drop for ModifierNodeHandle {
    fn drop(&mut self) {
        if !self.attached.get() {
            return;
        }
        // `attached` is shared by every handle that adopted the same node, so
        // the last reference is the one leaving the modifier tree. Node clones
        // held by a frame's hit regions must not keep it attached.
        if Rc::strong_count(&self.attached) != 1 {
            return;
        }
        self.attached.set(false);
        if let Some(node) = self.node.borrow().as_ref() {
            node.on_detach();
        }
    }
}
