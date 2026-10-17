/// Pointer confinement for the host window, the `winit` and Bevy
/// `CursorGrabMode` set. Apps declare it per frame through
/// [`cursor_grab`](crate::runtime::Scheduler::cursor_grab).
///
/// `Confined` is the one that means "the mouse cannot escape the
/// window": the OS keeps the pointer inside its bounds while the
/// cursor stays visible. `Locked` additionally hides it and reports
/// movement without bounds (FPS mouselook); platforms that cannot
/// lock fall back to confining in the runners.
///
/// The web is the odd one out: `Locked` drives pointer lock (which a
/// browser grants only after a user gesture) and `Confined` has no
/// equivalent, so it stays free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorGrabMode {
    /// The pointer moves freely, in and out of the window.
    #[default]
    None,
    /// The pointer cannot leave the window. Still drawn and still
    /// reaches every pixel inside, so absolute-position controls
    /// (a paddle, a drag) keep working at the window edge.
    Confined,
    /// The pointer is hidden and pinned to the window, reporting
    /// relative movement only.
    Locked,
}

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
