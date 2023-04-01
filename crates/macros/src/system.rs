use std::collections::HashMap;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, GenericArgument, Pat, PathArguments, Type};

// var name -> inner type
enum SystemParam<'a> {
    ContextRef,
    ResourceRef(&'a Type),
    StateRef(&'a Type),
}

impl<'a> SystemParam<'a> {
    pub fn is_mutable(&self) -> bool {
        false
    }
}

// if is_var_mut {
//                 is_ctx_mut = true;

//                 quote! {
//                     let mut #var_name = #var_value;
//                 }
//             } else {
//                 quote! {
//                     let #var_name = #var_value;
//                 }
//             }

pub fn macro_impl(_args: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as syn::ItemFn);
    let func_name = func.sig.ident;
    let func_body = func.block;

    // Convert inputs to system param enums
    let mut mut_call_count = 0;
    let params = func
        .sig
        .inputs
        .iter()
        .map(|i| {
            let FnArg::Typed(input) = i else {
                panic!("&self not permitted in system functions.");
            };

            let var_name = match input.pat.as_ref() {
                Pat::Ident(ref pat) => &pat.ident,
                _ => panic!("Unsupported parameter identifier pattern."),
            };

            let var_value = match input.ty.as_ref() {
                Type::Path(ref path) => {
                    // TypeWrapper<InnerType>
                    let segment = path
                        .path
                        .segments
                        .first()
                        .unwrap_or_else(|| panic!("Required a parameter type for {}.", var_name));

                    // TypeWrapper
                    let type_wrapper = segment.ident.to_string();

                    let param = if segment.arguments.is_empty() {
                        match type_wrapper.as_ref() {
                            "ContextRef" => SystemParam::ContextRef,
                            wrapper => {
                                panic!("Unknown parameter type {} for {}.", wrapper, var_name);
                            }
                        }
                    } else {
                        // <InnerType>
                        let PathArguments::AngleBracketed(segment_args) = &segment.arguments else {
                            panic!("Required a generic parameter type for {}.", type_wrapper);
                        };

                        // InnerType
                        let GenericArgument::Type(inner_type) = segment_args.args.first().unwrap() else {
                            panic!("Required a generic parameter type for {}.", type_wrapper);
                        };

                        match type_wrapper.as_ref() {
                            "ResourceRef" => SystemParam::ResourceRef(inner_type),
                            "StateRef" => SystemParam::StateRef(inner_type),
                            wrapper => {
                                panic!("Unknown parameter type {} for {}.", wrapper, var_name);
                            }
                        }
                    };

                    if param.is_mutable() {
                        mut_call_count += 1;
                    }

                    param
                }
                _ => panic!("Unsupported parameter type for {}.", var_name),
            };

            (var_name, var_value)
        })
        .collect::<HashMap<_, _>>();

    // When using mutable params, only 1 is allowed because of borrow rules
    if mut_call_count > 1 {
        panic!("Only 1 mutable parameter is allowed per system function.");
    }

    if params.len() > 1 {
        // When using `ContextRef`, only 1 param is allowed as it takes precedence
        if params
            .iter()
            .any(|(_, p)| matches!(p, SystemParam::ContextRef))
        {
            panic!("No additional parameters are allowed when using ContextRef.");
        }
    }

    // Convert system params to context calls
    let mut ctx_var_name = format_ident!("ctx");
    let ctx_calls = params
        .iter()
        .map(|(k, p)| match p {
            SystemParam::ContextRef => {
                ctx_var_name = (*k).to_owned();
                quote! {}
            }
            SystemParam::ResourceRef(inner) => quote! {
                let #k = ctx.resource::<#inner>();
            },
            SystemParam::StateRef(inner) => quote! {
                let #k = ctx.state::<#inner>();
            },
        })
        .collect::<Vec<_>>();

    let ctx_lock = if mut_call_count > 0 {
        quote! { let mut #ctx_var_name = ctx.write().await; }
    } else {
        quote! { let #ctx_var_name = ctx.read().await; }
    };

    quote! {
        async fn #func_name(ctx: starship::Context) -> starship::SystemResult {
            #ctx_lock
            #(#ctx_calls)*
            #func_body
            Ok(())
        }
    }
    .into()
}
