use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[derive(FromDeriveInput, Default)]
#[darling(default, attributes(my_trait))]
struct EventArgs {
    answer: Option<i32>,
}

pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);
    let args = EventArgs::from_derive_input(&input);

    quote! {
        impl starship::Event for #ident {
            type Value = usize;
        }
    }
    .into()
}
