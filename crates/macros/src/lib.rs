mod event;
mod resource;
mod state;
mod subscriber;
mod system;

use proc_macro::TokenStream;

#[proc_macro_derive(Event, attributes(event))]
pub fn event(item: TokenStream) -> TokenStream {
    event::macro_impl(item)
}

#[proc_macro_attribute]
pub fn subscriber(args: TokenStream, item: TokenStream) -> TokenStream {
    subscriber::macro_impl(args, item)
}

#[proc_macro_derive(Resource)]
pub fn resource(item: TokenStream) -> TokenStream {
    resource::macro_impl(item)
}

#[proc_macro_derive(State)]
pub fn state(item: TokenStream) -> TokenStream {
    state::macro_impl(item)
}

#[proc_macro_attribute]
pub fn system(args: TokenStream, item: TokenStream) -> TokenStream {
    system::macro_impl(args, item)
}
