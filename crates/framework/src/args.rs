use rustc_hash::FxHashMap;
use starbase_macros::State;
use std::any::{type_name, Any, TypeId};
use std::fmt::Debug;

#[derive(Debug, Default)]
pub struct ArgsMap {
    cache: FxHashMap<TypeId, Box<dyn Any + Sync + Send>>,
}

impl ArgsMap {
    /// Get an immutable args reference for the provided type.
    /// If the args does not exist, a panic will be triggered.
    pub fn get<T: Any + Send + Sync>(&self) -> &T {
        if let Some(value) = self.cache.get(&TypeId::of::<T>()) {
            return value.downcast_ref::<T>().unwrap();
        }

        panic!("{} does not exist!", type_name::<T>())
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

#[derive(State)]
pub struct Args(pub ArgsMap);
