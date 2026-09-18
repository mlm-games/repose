#[derive(Clone, Debug, PartialEq, Eq)]
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
    Custom(std::sync::Arc<CustomCursorImage>),
}

/// Pixel payload for [`CursorIcon::Custom`].
#[derive(Clone, PartialEq, Eq)]
pub struct CustomCursorImage {
    /// Straight-alpha RGBA bytes, `w * h * 4` long.
    pub rgba: std::sync::Arc<[u8]>,
    /// Image dimensions in px.
    pub size: [u16; 2],
    /// Hotspot (click point) in px from the top-left.
    pub hotspot: [u16; 2],
}

impl std::fmt::Debug for CustomCursorImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomCursorImage")
            .field("size", &self.size)
            .field("hotspot", &self.hotspot)
            .field("rgba_len", &self.rgba.len())
            .finish()
    }
}
