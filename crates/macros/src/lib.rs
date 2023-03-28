mod event;
mod listener;
mod state;

use proc_macro::TokenStream;

#[proc_macro_derive(Event, attributes(event))]
pub fn event(item: TokenStream) -> TokenStream {
    event::macro_impl(item)
}

#[proc_macro_attribute]
pub fn listener(args: TokenStream, item: TokenStream) -> TokenStream {
    listener::macro_impl(args, item)
}

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::macro_impl(item)
}
