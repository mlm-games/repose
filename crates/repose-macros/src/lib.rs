use proc_macro::TokenStream;

/// Placeholder for later v0.2+.
/// Intentionally does nothing in v0.1 to keep builds fast.
#[proc_macro]
pub fn view(_input: TokenStream) -> TokenStream {
    "{}".parse().unwrap()
}
