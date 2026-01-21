#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorIcon {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    Grab,
    Grabbing,
}
