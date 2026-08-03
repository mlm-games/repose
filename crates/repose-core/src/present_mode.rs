/// Preferred GPU present mode for windowed rendering.
///
/// The renderer picks the closest supported mode from the surface
/// capabilities, falling back to an "auto" Fifo-first selection when the
/// preferred mode is unavailable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PresentModePref {
    /// Limit to display refresh using vsync: prefer Fifo, then Mailbox, then
    /// Immediate. This is the default and matches the renderer's historical
    /// behavior.
    #[default]
    Auto,
    /// Vsync, always wait for the next vblank (no tearing). Falls back to
    /// `Auto` if unsupported.
    Fifo,
    /// Low latency, waits for the vblank only when a new frame is available
    /// (no tearing). Falls back to `Auto` if unsupported.
    Mailbox,
    /// Present immediately without waiting (may tear). Useful for uncapped
    /// low-latency rendering. Falls back to `Auto` if unsupported.
    Immediate,
}
