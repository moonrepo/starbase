use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};

pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);

    let empty_impl: TokenStream = quote! {}.into();
    let struct_name = input.ident;

    match input.data {
        Data::Struct(_) => quote! {
             impl AsRef<#struct_name> for #struct_name {
                fn as_ref(&self) -> &#struct_name {
                    self
                }
            }
        }
        .into(),
        Data::Enum(_) => empty_impl,
        Data::Union(_) => panic!("#[derive(Resource)] is not supported for unions."),
    }
}
