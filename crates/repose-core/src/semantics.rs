/// High‑level semantic role of a view, similar to ARIA roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Text,
    Button,
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
            // value: None,
            // checked: None,
        }
    }
}
