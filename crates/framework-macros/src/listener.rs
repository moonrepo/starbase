use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, Pat, Type, TypePath};

// #[listener]
pub fn macro_impl(_args: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as syn::ItemFn);
    let func_name = func.sig.ident;
    let func_body = func.block;

    let FnArg::Typed(event_param) = func
        .sig
        .inputs
        .first()
        .expect("Requires an event as the only parameter.") else {
        panic!("Unsupported param type.");
    };

    let Pat::Ident(event_param_name) = event_param.pat.as_ref() else {
        panic!("Unsupported param, must be an identifier.");
    };

    let event_name = &event_param_name.ident;
    let mut event_type = TypePath::from_string("Event").unwrap();
    let mut is_mutable = event_param_name.mutability.is_some();

    match event_param.ty.as_ref() {
        Type::Path(path) => {
            event_type = path.to_owned();
        }
        Type::Reference(refs) => {
            if refs.mutability.is_some() {
                is_mutable = true;
            }

            if let Type::Path(ref_path) = refs.elem.as_ref() {
                event_type = ref_path.to_owned();
            }
        }
        _ => {
            panic!("Unsupported event param type, must be a path or reference.");
        }
    };

    let acquire_lock = if is_mutable {
        quote! { let mut #event_name = #event_name.write().await; }
    } else {
        quote! { let #event_name = #event_name.read().await; }
    };

    quote! {
        async fn #func_name(
            #event_name: std::sync::Arc<tokio::sync::RwLock<#event_type>>
        ) -> EventResult<#event_type> {
            #acquire_lock
            #func_body
        }
    }
    .into()
}
