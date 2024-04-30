use crate::states::StateInstance;
use std::any::{Any, TypeId};
use std::fmt::Debug;

#[derive(Debug, Default)]
pub struct ArgsMap {
    cache: scc::HashMap<TypeId, Box<dyn Any + Sync + Send>>,
}

impl ArgsMap {
    /// Get an immutable args reference for the provided type.
    /// If the args does not exist, a [`None`] is returned.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        if let Some(entry) = self.cache.get(&TypeId::of::<T>()) {
            return entry.get().downcast_ref::<T>();
        }

        None
    }

    /// Set the args into the registry with the provided type.
    /// If an exact type already exists, it'll be overwritten.
    pub fn set<T: Any + Send + Sync>(&self, args: T) {
        self.cache.insert(TypeId::of::<T>(), Box::new(args));
    }
}

#[derive(Debug)]
pub struct ExecuteArgs(pub ArgsMap);

impl StateInstance for ExecuteArgs {
    fn extract<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.0.get::<T>()
    }
}
