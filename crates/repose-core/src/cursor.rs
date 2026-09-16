#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorIcon {
    Default,
    Pointer,
    Text,
    EwResize,
    NsResize,
    NwseResize,
    NeswResize,
    Grab,
    Grabbing,
    /// Hide the OS cursor entirely. Games draw their own crosshair
    /// (e.g. twin-stick shooters in keyboard mode) and the OS arrow
    /// would double-paint next to it. Platform runners implement this
    /// with `set_cursor_visible(false)`, not an icon.
    Hidden,
}
