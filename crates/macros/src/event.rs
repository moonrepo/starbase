use darling::FromDeriveInput;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, ExprPath};

#[derive(FromDeriveInput, Default)]
#[darling(default, attributes(event))]
struct EventArgs {
    dataset: Option<ExprPath>,
}

// #[derive(Event)]
// #[event]
// #[event(data = String)]
pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);
    let args = EventArgs::from_derive_input(&input).expect("Failed to parse arguments.");

    let struct_name = input.ident;
    let data_type = match args.dataset {
        Some(value) => quote! { #value },
        None => quote! { () },
    };

    quote! {
        #[automatically_derived]
        impl starbase::Event for #struct_name {
            type Data = #data_type;
        }
    }
    .into()
}
