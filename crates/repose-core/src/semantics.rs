/// High‑level semantic role of a view, similar to ARIA roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Text,
    Button,
    Tab,
    TextField,
    Container,
    Checkbox,
    RadioButton,
    Switch,
    Slider,
    ProgressBar,
}

/// Semantics attached to a `View`, used to build the accessibility tree.
#[derive(Clone, Debug)]
pub struct Semantics {
    /// Primary role of this node (what kind of thing it is).
    pub role: Role,
    /// Human‑readable label for screen readers. For buttons, this is the
    /// “name” that is announced.
    pub label: Option<String>,
    /// Whether this node is currently focused.
    pub focused: bool,
    /// Whether this node is actionable; disabled nodes remain in the tree
    /// but are marked not enabled.
    pub enabled: bool,
    /// Marks this node as a collection of horizontally or vertically stacked
    /// selectable elements (ex: Tabs, RadioButtons).
    pub selectable_group: bool,
    // pub value: Option<String>,
    // pub checked: Option<bool>,
}

impl Semantics {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            label: None,
            focused: false,
            enabled: true,
            selectable_group: false,
            // value: None,
            // checked: None,
        }
    }
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn with_selectable_group(mut self) -> Self {
        self.selectable_group = true;
        self
    }
}
