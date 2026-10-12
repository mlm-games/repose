/// High‑level semantic role of a view, similar to ARIA roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    Text,
    Button,
    Tab,
    TextField,
    #[default]
    Container,
    Checkbox,
    RadioButton,
    Switch,
    Slider,
    ProgressBar,
    Dialog,
    Image,
    DropdownList,
    ValuePicker,
    Carousel,
}

/// Live-region politeness for screen-reader announcements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LiveRegion {
    #[default]
    Polite,
    Assertive,
}

/// Numeric range info for sliders and progress indicators.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressRangeInfo {
    pub current: f32,
    pub min: f32,
    pub max: f32,
    pub step: Option<f32>,
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
    /// Whether this node is actionable. Disabled nodes remain in the tree
    /// but are marked not enabled.
    pub enabled: bool,
    /// Marks this node as a collection of horizontally or vertically stacked
    /// selectable elements (ex: Tabs, RadioButtons).
    pub selectable_group: bool,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub value: Option<String>,
    pub heading: bool,
    pub live_region: Option<LiveRegion>,
    pub state_description: Option<String>,
    pub traversal_index: Option<f32>,
    pub is_traversal_group: bool,
    pub hidden: bool,
    pub collection_rows: Option<usize>,
    pub collection_columns: Option<usize>,
    pub row_index: Option<usize>,
    pub column_index: Option<usize>,
    pub progress: Option<ProgressRangeInfo>,
    pub error: Option<String>,
    pub expanded: Option<bool>,
}

impl Semantics {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            label: None,
            focused: false,
            enabled: true,
            selectable_group: false,
            checked: None,
            selected: None,
            value: None,
            heading: false,
            live_region: None,
            state_description: None,
            traversal_index: None,
            is_traversal_group: false,
            hidden: false,
            collection_rows: None,
            collection_columns: None,
            row_index: None,
            column_index: None,
            progress: None,
            error: None,
            expanded: None,
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
    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    pub fn with_heading(mut self) -> Self {
        self.heading = true;
        self
    }
    pub fn with_live_region(mut self, region: LiveRegion) -> Self {
        self.live_region = Some(region);
        self
    }
    pub fn with_state_description(mut self, description: impl Into<String>) -> Self {
        self.state_description = Some(description.into());
        self
    }
    pub fn with_traversal_index(mut self, index: f32) -> Self {
        self.traversal_index = Some(index);
        self
    }
    pub fn with_traversal_group(mut self) -> Self {
        self.is_traversal_group = true;
        self
    }
    pub fn with_hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
    pub fn with_collection(mut self, rows: usize, columns: usize) -> Self {
        self.collection_rows = Some(rows);
        self.collection_columns = Some(columns);
        self
    }
    pub fn with_collection_item(mut self, row: usize, column: usize) -> Self {
        self.row_index = Some(row);
        self.column_index = Some(column);
        self
    }
    pub fn with_progress_range(mut self, info: ProgressRangeInfo) -> Self {
        self.progress = Some(info);
        self
    }
    pub fn with_error(mut self, message: impl Into<String>) -> Self {
        self.error = Some(message.into());
        self
    }
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
}

impl Default for Semantics {
    fn default() -> Self {
        Self::new(Role::default())
    }
}
