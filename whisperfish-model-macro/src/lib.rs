extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;

mod observing_model;
use observing_model::*;

fn parse_until<E: syn::parse::Peek>(
    input: syn::parse::ParseStream,
    end: E,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut tokens = proc_macro2::TokenStream::new();
    while !input.is_empty() && !input.peek(end) {
        let next: proc_macro2::TokenTree = input.parse()?;
        tokens.extend(Some(next));
    }
    Ok(tokens)
}

#[proc_macro_attribute]
pub fn observing_model(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut strukt = syn::parse_macro_input!(input as syn::ItemStruct);
    let attr = syn::parse_macro_input!(attr as ObservingModelAttribute);

    let syn::Fields::Named(fields) = &mut strukt.fields else {
        return TokenStream::from(quote! {
            compile_error!("expected a struct with named fields");
        });
    };

    inject_struct_fields(&attr, fields);
    let methods = generate_methods(&attr, &strukt);
    let property_wrappers = inject_literal_properties(&mut strukt);

    TokenStream::from(quote! {
        #strukt
        #methods
        #property_wrappers
    })
}
