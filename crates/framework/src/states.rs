use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::{
    any::{type_name, Any, TypeId},
    sync::Arc,
};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

create_instance_manager!(StateManager, StateInstance);

pub type States = Arc<RwLock<StateManager>>;

// Not used directly but provides types for #[system] macro
pub type StatesRef = RwLockReadGuard<'static, StateManager>;
pub type StatesMut = RwLockWriteGuard<'static, StateManager>;
pub type StateRef<T> = &'static T;
pub type StateMut<T> = &'static mut T;
