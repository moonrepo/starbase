use std::any::Any;

// Does nothing at the moment besides type guarding `ContextManager` methods.
pub trait StateInstance: Any {}
