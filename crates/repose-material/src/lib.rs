pub mod material3;
pub mod ripple;
mod symbol;
pub use symbol::Symbol;

use repose_core::View;
use repose_ui::{Text, TextStyle};

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
    Text(symbol.codepoint.to_string()).font_family("Material Symbols Outlined")
}
