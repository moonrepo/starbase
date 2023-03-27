use convert_case::{Case, Casing};
use darling::FromMeta;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, AttributeArgs, FnArg, Type};

#[derive(Debug, FromMeta)]
struct ListenerArgs {
    #[darling(default)]
    once: bool,
}

pub fn macro_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let func = parse_macro_input!(item as syn::ItemFn);
    let args = parse_macro_input!(args as AttributeArgs);
    let args = ListenerArgs::from_list(&args).expect("Failed to parse #[listener] arguments.");

    let func_name = func.sig.ident.to_string();
    let func_body = func.block;
    let listener_name = format_ident!("{}Listener", func_name.to_case(Case::Pascal));
    let is_once = format_ident!("{}", if args.once { "true" } else { "false" });

    // Extract event name
    let event_name = match func
        .sig
        .inputs
        .first()
        .expect("Macro #[listener] requires an event as the only argument.")
    {
        FnArg::Receiver(_) => panic!("Cannot use &self as an event."),
        FnArg::Typed(arg) => match &*arg.ty {
            Type::Reference(inner) => match &*inner.elem {
                Type::Path(path) => path.path.get_ident().unwrap().to_owned(),
                Type::Verbatim(tokens) => format_ident!("{}", tokens.to_string()),
                _ => panic!("Requires a literal event name."),
            },
            _ => panic!("Requires a mutable referenced event."),
        },
    };

    quote! {
        #[derive(Debug)]
        struct #listener_name;

        #[async_trait::async_trait]
        #[automatically_derived]
        impl starship::Listener<#event_name> for #listener_name {
            fn is_once(&self) -> bool {
                #is_once
            }

            async fn on_emit(&mut self, event: &mut #event_name) -> starship::EventResult<#event_name> {
                #func_body
            }
        }
    }
    .into()
}
