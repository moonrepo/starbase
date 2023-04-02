use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::{
    any::{type_name, Any, TypeId},
    sync::Arc,
};
use tokio::sync::RwLock;

create_instance_manager!(StateManager, StateInstance);

pub type States = Arc<RwLock<StateManager>>;
