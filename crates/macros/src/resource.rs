use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

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
        Data::Struct(data) => {
            let mut impls = vec![
                shared_impl,
                quote! {
                    #[automatically_derived]
                    impl AsRef<#struct_name> for #struct_name {
                        fn as_ref(&self) -> &#struct_name {
                            self
                        }
                    }
                },
            ];

            match &data.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                    let inner = fields.unnamed.first().unwrap();
                    let inner_type = &inner.ty;

                    impls.push(quote! {
                        #[automatically_derived]
                        impl std::ops::Deref for #struct_name {
                            type Target = #inner_type;

                            fn deref(&self) -> &Self::Target {
                                &self.0
                            }
                        }

                        #[automatically_derived]
                        impl std::ops::DerefMut for #struct_name {
                            fn deref_mut(&mut self) -> &mut Self::Target {
                                &mut self.0
                            }
                        }

                        #[automatically_derived]
                        impl AsRef<#inner_type> for #struct_name {
                            fn as_ref(&self) -> &#inner_type {
                                &self.0
                            }
                        }
                    });
                }
                _ => {}
            }

            quote! {
                #(#impls)*
            }
            .into()
        }
        Data::Enum(_) => shared_impl.into(),
        Data::Union(_) => panic!("#[derive(Resource)] is not supported for unions."),
    }
}
