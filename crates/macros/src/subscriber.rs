use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, ExprCall, ExprPath, FnArg, Pat, Stmt, Type, TypePath, parse_macro_input};

fn is_event_state(path: &ExprPath) -> bool {
    let Some(state) = path.path.segments.first() else {
        return false;
    };

    state.ident == "EventState"
}

fn is_return_event_state(call: &ExprCall) -> bool {
    // Ok(_), Err(_)
    let Expr::Path(func) = call.func.as_ref() else {
        return false;
    };

    let ident = func.path.get_ident().unwrap();

    if ident == "Err" {
        return true;
    }

    if ident != "Ok" {
        return false;
    }

    match call.args.first() {
        // EventState::Continue
        // EventState::Stop
        Some(Expr::Path(arg)) => is_event_state(arg),
        // EventState::Return(_)
        Some(Expr::Call(call)) => match call.func.as_ref() {
            Expr::Path(func) => is_event_state(func),
            _ => false,
        },
        _ => false,
    }
}

fn has_return_statement(block: &syn::Block) -> bool {
    let Some(last_statement) = block.stmts.last() else {
        return false;
    };

    let expr = match &last_statement {
        // value
        // return value;
        Stmt::Expr(expr, _) => expr,
        _ => {
            return false;
        }
    };

    match expr {
        // Ok(_)
        Expr::Call(call) => is_return_event_state(call),
        // return Ok(_);
        Expr::Return(ret) => match ret.expr.as_ref() {
            Some(expr) => match expr.as_ref() {
                Expr::Call(call) => is_return_event_state(call),
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

// #[subscriber]
pub fn macro_impl(_args: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as syn::ItemFn);
    let func_name = func.sig.ident;
    let func_body = func.block;

    let FnArg::Typed(event_param) = func
        .sig
        .inputs
        .first()
        .expect("Requires an event as the only parameter.")
    else {
        panic!("Unsupported param type.");
    };

    let Pat::Ident(event_param_name) = event_param.pat.as_ref() else {
        panic!("Unsupported param, must be an identifier.");
    };

    let data_name = &event_param_name.ident;
    let mut event_type = TypePath::from_string("Event").unwrap();
    let mut is_mutable = event_param_name.mutability.is_some();

    match event_param.ty.as_ref() {
        Type::Path(path) => {
            path.clone_into(&mut event_type);
        }
        Type::Reference(refs) => {
            if refs.mutability.is_some() {
                is_mutable = true;
            }

            if let Type::Path(ref_path) = refs.elem.as_ref() {
                ref_path.clone_into(&mut event_type);
            }
        }
        _ => {
            panic!("Unsupported event param type, must be a path or reference.");
        }
    };

    let acquire_lock = if is_mutable {
        quote! { let mut #data_name = #data_name.write().await; }
    } else {
        quote! { let #data_name = #data_name.read().await; }
    };

    let return_flow = if has_return_statement(&func_body) {
        quote! {}
    } else {
        quote! { Ok(starbase_events::EventState::Continue) }
    };

    let attributes = if cfg!(feature = "tracing") {
        quote! {
            #[tracing::instrument(skip_all)]
        }
    } else {
        quote! {}
    };

    quote! {
        #attributes
        async fn #func_name(
            event: std::sync::Arc<#event_type>,
            #data_name: std::sync::Arc<tokio::sync::RwLock<<#event_type as starbase_events::Event>::Data>>
        ) -> starbase_events::EventResult {
            #acquire_lock
            #func_body
            #return_flow
        }
    }
    .into()
}
