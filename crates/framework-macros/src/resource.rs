use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};

// #[derive(Resource)]
pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);
    let struct_name = input.ident;

    let shared_impl = quote! {
        #[automatically_derived]
        impl starbase::ResourceInstance for #struct_name {
        }
    };

    match input.data {
        Data::Struct(_) => quote! {
            #shared_impl

            #[automatically_derived]
            impl AsRef<#struct_name> for #struct_name {
                fn as_ref(&self) -> &#struct_name {
                    self
                }
            }
        }
        .into(),
        Data::Enum(_) => shared_impl.into(),
        Data::Union(_) => panic!("#[derive(Resource)] is not supported for unions."),
    }
}
