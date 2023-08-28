use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::{
    any::{type_name, Any, TypeId},
    sync::Arc,
};
use tokio::sync::RwLock;

create_instance_manager!(StateManager, StateInstance, {
    /// Extract the provided type from the state instance.
    /// If the type does not exist, or the state does not support
    /// extraction, [`None`] will be returned.
    fn extract<T: Any + Send + Sync>(&self) -> Option<&T> {
        None
    }
});

pub type States = Arc<RwLock<StateManager>>;
