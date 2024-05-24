use crate::instance::{BoxedAnyInstance, InstanceGuard};
use crate::states::StateInstance;
use std::any::{Any, TypeId};
use std::fmt::Debug;
use std::sync::Arc;

// Don't use `create_instance_manager` because we have no trait for
// values to implement! We also return `None` instead of panicing.
#[derive(Debug, Default)]
pub struct ArgsMap {
    cache: scc::HashMap<TypeId, BoxedAnyInstance>,
}

impl ArgsMap {
    /// Get an args reference for the provided type.
    /// If the args does not exist, a [`None`] is returned.
    pub async fn get<T: Any + Send + Sync>(&self) -> Option<InstanceGuard<T>> {
        if let Some(entry) = self.cache.get_async(&TypeId::of::<T>()).await {
            return Some(InstanceGuard::new(entry));
        }

        None
    }

    /// Set the args into the registry with the provided type.
    /// If an exact type already exists, it'll be overwritten.
    pub async fn set<T: Any + Send + Sync>(&self, args: T) {
        let _ = self
            .cache
            .insert_async(TypeId::of::<T>(), Box::new(args))
            .await;
    }

    #[doc(hidden)]
    pub(crate) fn set_sync<T: Any + Send + Sync>(&self, args: T) {
        let _ = self.cache.insert(TypeId::of::<T>(), Box::new(args));
    }
}

#[derive(Debug)]
pub struct ExecuteArgs(pub Arc<ArgsMap>);

impl StateInstance for ExecuteArgs {
    // async fn extract<T: Any + Clone + Send + Sync>(&self) -> Option<T> {
    //     self.0.get::<T>().await.map(|i| i.read().to_owned())
    // }
}
