use anyhow::anyhow;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};
use tokio::sync::RwLock;

pub type Instance = Box<dyn Any + Sync + Send>;

pub struct InstanceRegistry {
    instances: FxHashMap<TypeId, RwLock<Instance>>,
}

impl InstanceRegistry {
    pub async fn get<C: Any + Send + Sync>(&self) -> anyhow::Result<&RwLock<Instance>> {
        self.instances
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No instance found for type {:?}", type_name::<C>()))
    }
}
