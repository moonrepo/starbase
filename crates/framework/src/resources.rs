use crate::create_instance_manager;
use std::{
    any::{type_name, Any, TypeId},
    sync::Arc,
};

create_instance_manager!(ResourceManager, ResourceInstance);

pub type Resources = Arc<ResourceManager>;
