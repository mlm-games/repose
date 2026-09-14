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
                } else if content.is_empty() {
                    break;
                } else {
                    return Err(syn::Error::new(
                        content.span(),
                        "expected `,` between modifier args",
                    ));
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
                } else if content.is_empty() {
                    break;
                } else {
                    return Err(syn::Error::new(
                        content.span(),
                        "expected `,` between children",
                    ));
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
    match syn::parse::<ViewMacro>(cloned) {
        Ok(m) => expand_view(m).into(),
        Err(macro_err) => {
            if let Ok(expr) = syn::parse::<Expr>(input) {
                return quote!(#expr).into();
            }
            macro_err.to_compile_error().into()
        }
    }
}

fn expand_view(m: ViewMacro) -> proc_macro2::TokenStream {
    let ViewMacro {
        layout,
        modifiers,
        children,
    } = m;

    let compile_err = || {
        syn::Error::new(
            proc_macro2::Span::call_site(),
            "View!: expected a single expression or `Layout(modifiers) { children }`",
        )
        .to_compile_error()
    };

    if children.is_empty() && modifiers.is_empty() {
        return compile_err();
    }

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
            compile_err()
        } else if let Some(layout) = layout {
            quote! {
                ::repose_ui::#layout(::repose_core::Modifier::new() #(#mod_calls)*)
            }
        } else {
            quote! {
                ::repose_ui::Column(::repose_core::Modifier::new() #(#mod_calls)*)
            }
        }
    } else if let Some(layout) = layout {
        // Layout with children
        let child_exprs = &children;
        quote! {
            ::repose_ui::#layout(::repose_core::Modifier::new() #(#mod_calls)*)
                .child((#(#child_exprs,)*))
        }
    } else {
        // Bare children without layout: wrap in Column
        let child_exprs = &children;
        let mod_tokens = if modifiers.is_empty() {
            quote!(::repose_core::Modifier::new())
        } else {
            quote!(::repose_core::Modifier::new() #(#mod_calls)*)
        };
        quote! {
            ::repose_ui::Column(#mod_tokens).child((#(#child_exprs,)*))
        }
    }
}
