#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Symbol {
    pub name: &'static str,
    pub codepoint: char,
}

impl Symbol {
    pub const fn new(name: &'static str, codepoint: char) -> Self {
        Self { name, codepoint }
    }
}
