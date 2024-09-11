extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn observing_model(attr: TokenStream, input: TokenStream) -> TokenStream {
    let _attr = proc_macro2::TokenStream::from(attr);
    let input = proc_macro2::TokenStream::from(input);

    input.into()
}
