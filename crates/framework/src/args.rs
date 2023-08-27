use crate::states::StateInstance;
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::fmt::Debug;

#[derive(Debug, Default)]
pub struct ArgsMap {
    cache: FxHashMap<TypeId, Box<dyn Any + Sync + Send>>,
}

impl ArgsMap {
    /// Get an immutable args reference for the provided type.
    /// If the args does not exist, a panic will be triggered.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        if let Some(value) = self.cache.get(&TypeId::of::<T>()) {
            return value.downcast_ref::<T>();
        }

        None
    }

    /// Set the args into the registry with the provided type.
    /// If an exact type already exists, it'll be overwritten.
    pub fn set<T: Any + Send + Sync>(&mut self, args: T) -> &mut Self {
        self.cache.insert(TypeId::of::<T>(), Box::new(args));
        self
    }
}

// This is a hack for starbase macros to work from within
// the starbase crate itself!
mod starbase {
    pub use crate::*;
}

#[derive(Debug)]
pub struct ExecuteArgs(pub ArgsMap);

impl StateInstance for ExecuteArgs {
    fn extract<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.0.get::<T>()
    }
}
