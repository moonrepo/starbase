use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

#[derive(FromDeriveInput, Default)]
#[darling(default, attributes(event))]
struct EventArgs {
    value: Option<Ident>,
}

pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);
    let args = EventArgs::from_derive_input(&input).expect("Failed to parse #[event] arguments.");

    let struct_name = input.ident;
    let value_type = match args.value {
        Some(value) => quote! { #value },
        None => quote! { () },
    };

    quote! {
        impl starship::Event for #struct_name {
            type Value = #value_type;
        }
    }
    .into()
}
