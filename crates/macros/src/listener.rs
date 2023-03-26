use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, FnArg, Ident, Token, Type};

struct ListenerArgs {
    local: bool,
}

impl Parse for ListenerArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let vars = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;

        Ok(ListenerArgs {
            local: !vars.is_empty() && vars[0].to_string() == "local",
        })
    }
}

pub fn macro_impl(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ListenerArgs);
    let func = parse_macro_input!(item as syn::ItemFn);

    let func_name = func.sig.ident.to_string();
    let func_body = func.block;
    let listener_name = format_ident!("{}Listener", func_name.to_case(Case::Pascal));
    let crate_scope = format_ident!("{}", if args.local { "crate" } else { "::starship" });

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
        impl #crate_scope::Listener<#event_name> for #listener_name {
            async fn on_emit(&mut self, event: &mut #event_name) -> #crate_scope::EventResult<#event_name> {
                #func_body
            }
        }
    }
    .into()
}
