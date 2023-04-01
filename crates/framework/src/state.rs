use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};

create_instance_manager!(StateManager, StateInstance);
