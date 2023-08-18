use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::{
    any::{type_name, Any, TypeId},
    sync::Arc,
};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

create_instance_manager!(ResourceManager, ResourceInstance);

pub type Resources = Arc<RwLock<ResourceManager>>;

// Not used directly but provides types for #[system] macro
pub type ResourcesRef = RwLockReadGuard<'static, ResourceManager>;
pub type ResourcesMut = RwLockWriteGuard<'static, ResourceManager>;
pub type ResourceRef<T> = &'static T;
pub type ResourceMut<T> = &'static mut T;
