pub mod material3;
mod symbol;
pub use symbol::Symbol;

use repose_core::View;
use repose_ui::Text;

/// Material Symbols font data (bundled).
static MATERIAL_SYMBOLS_TTF: &[u8] = include_bytes!("assets/MaterialSymbolsOutlined.ttf");

/// Ensures Material Symbols font is registered. Called automatically on first icon use.
fn ensure_font_registered() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        repose_text::register_font_data(MATERIAL_SYMBOLS_TTF);
    });
}

/// Register a font blob into the global FontSystem.
pub fn install_material_symbols_font(bytes: &'static [u8]) {
    repose_text::register_font_data(bytes);
}

/// Declare a set of Material Symbols by name.
#[macro_export]
macro_rules! material_symbols {
    ( $($name:ident : $ch:literal),* $(,)? ) => {
        pub struct Symbols;
        impl Symbols {
            $(
                pub const $name: $crate::Symbol =
                    $crate::Symbol::new(stringify!($name), $ch);
            )*
        }
    };
}

/// A Material Symbol icon.
pub fn Icon(symbol: Symbol) -> View {
    ensure_font_registered();
    Text(symbol.codepoint.to_string())
}
