mod listener;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn listener(args: TokenStream, item: TokenStream) -> TokenStream {
    listener::macro_impl(args, item)
}
