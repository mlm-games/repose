use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Expr, Ident, Token, braced,
    parse::{Parse, ParseStream},
};

struct ViewMacro {
    layout: Option<Ident>,
    modifiers: Vec<(Ident, Option<Expr>)>,
    children: Vec<Expr>,
}

impl Parse for ViewMacro {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // If it's a single expression, treat as pass-through
        if input.peek(syn::token::Paren) || input.peek(syn::token::Bracket) {
            return Err(syn::Error::new(input.span(), "unexpected delimiters"));
        }

        // Parse optional layout identifier (followed by either { or ( )
        let layout = if input.peek(Ident)
            && (input.peek2(syn::token::Brace) || input.peek2(syn::token::Paren))
        {
            let ident: Ident = input.parse()?;
            Some(ident)
        } else {
            None
        };

        // Parse optional modifier args: (key: val, ...)
        let modifiers = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let mut mods = Vec::new();
            while !content.is_empty() {
                let name: Ident = content.parse()?;
                let value = if content.peek(Token![:]) {
                    content.parse::<Token![:]>()?;
                    Some(content.parse::<Expr>()?)
                } else {
                    None
                };
                mods.push((name, value));
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                } else {
                    break;
                }
            }
            mods
        } else {
            Vec::new()
        };

        // Parse children block: { expr, expr, ... }
        let children = if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let mut kids = Vec::new();
            while !content.is_empty() {
                let expr: Expr = content.parse()?;
                kids.push(expr);
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                } else {
                    break;
                }
            }
            kids
        } else {
            Vec::new()
        };

        Ok(Self {
            layout,
            modifiers,
            children,
        })
    }
}

/// A view tree builder macro.
///
/// # Example
///
/// ```ignore
/// // Pass-through single expression:
/// View!(Text("hello"))
///
/// // Layout with children:
/// View! {
///     Column {
///         Text("Hello"),
///         Text("World"),
///     }
/// }
///
/// // With modifier args:
/// View! {
///     Column(padding: 16.0, gap: 8.0) {
///         Text("Hello"),
///         Text("World"),
///     }
/// }
/// ```
#[proc_macro]
#[allow(non_snake_case)]
pub fn View(input: TokenStream) -> TokenStream {
    // Try ViewMacro parser first (handles `Ident { ... }` and `Ident(m: v) { ... }`)
    let cloned = input.clone();
    if let Ok(m) = syn::parse::<ViewMacro>(cloned) {
        return expand_view(m).into();
    }

    // Fallback: pass-through single expression
    if let Ok(expr) = syn::parse::<Expr>(input.clone()) {
        return quote!(#expr).into();
    }

    quote!({}).into()
}

fn expand_view(m: ViewMacro) -> proc_macro2::TokenStream {
    let ViewMacro {
        layout,
        modifiers,
        children,
    } = m;

    if children.is_empty() && modifiers.is_empty() {
        return quote!({});
    }

    let layout_name = layout.as_ref().map(|i| i.to_string()).unwrap_or_default();

    let mod_calls = modifiers.iter().map(|(name, value)| {
        if let Some(val) = value {
            quote!(.#name(#val))
        } else {
            quote!(.#name())
        }
    });

    if children.is_empty() {
        // Layout with modifiers but no children
        if modifiers.is_empty() {
            quote!({})
        } else {
            quote! {
                repose_ui::#layout_name(repose_core::Modifier::new() #(#mod_calls)*)
            }
        }
    } else if layout.is_some() {
        // Layout with children
        let child_exprs = &children;
        quote! {
            repose_ui::#layout_name(repose_core::Modifier::new() #(#mod_calls)*)
                .child((#(#child_exprs,)*))
        }
    } else {
        // Bare children without layout: wrap in Column
        let child_exprs = &children;
        quote! {
            repose_ui::Column(repose_core::Modifier::new()).child((#(#child_exprs,)*))
        }
    }
}
