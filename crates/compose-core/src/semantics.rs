#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Text,
    Button,
    TextField,
    Container,
}

#[derive(Clone, Debug)]
pub struct Semantics {
    pub role: Role,
    pub label: Option<String>,
    pub focused: bool,
    pub enabled: bool,
}

impl Semantics {
    pub fn new(role: Role) -> Self {
        Self { role, label: None, focused: false, enabled: true }
    }
}
