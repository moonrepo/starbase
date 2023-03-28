use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Type};

pub fn macro_impl(item: TokenStream) -> TokenStream {
    let input: DeriveInput = parse_macro_input!(item);
    let struct_name = input.ident;

    let shared_impl = quote! {
        impl starship::State for #struct_name {
        }
    };

    match input.data {
        Data::Struct(data) => {
            match data.fields {
                // Struct { field }
                Fields::Named(_) => quote! {
                    #shared_impl

                    impl AsRef<#struct_name> for #struct_name {
                        fn as_ref(&self) -> &#struct_name {
                            self
                        }
                    }
                }
                .into(),

                // Struct(inner)
                Fields::Unnamed(fields) => {
                    let inner = fields
                        .unnamed
                        .first()
                        .expect("#[derive(State)] on a struct requires a single unnamed field.");
                    let inner_type = &inner.ty;

                    let as_ref_extra = match inner_type {
                        Type::Path(path) => {
                            let is_pathbuf = path
                                .path
                                .get_ident()
                                .map(|i| i == "PathBuf")
                                .unwrap_or_default();

                            // When the inner type is a `PathBuf`, we must also implement
                            // `AsRef<Path>` for references to work correctly.
                            if is_pathbuf {
                                Some(quote! {
                                    impl AsRef<std::path::Path> for #struct_name {
                                        fn as_ref(&self) -> &std::path::Path {
                                            &self.0
                                        }
                                    }
                                })
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    quote! {
                        #shared_impl

                        impl std::ops::Deref for #struct_name {
                            type Target = #inner_type;

                            fn deref(&self) -> &Self::Target {
                                &self.0
                            }
                        }

                        impl std::ops::DerefMut for #struct_name {
                            fn deref_mut(&mut self) -> &mut Self::Target {
                                &mut self.0
                            }
                        }

                        impl AsRef<#inner_type> for #struct_name {
                            fn as_ref(&self) -> &#inner_type {
                                &self.0
                            }
                        }

                        #as_ref_extra
                    }
                    .into()
                }

                // Struct
                Fields::Unit => shared_impl.into(),
            }
        }
        Data::Enum(_) => shared_impl.into(),
        Data::Union(_) => panic!("#[derive(State)] is not supported for unions."),
    }
}
