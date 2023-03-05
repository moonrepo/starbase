use anyhow::anyhow;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};

pub type Instance = Box<dyn Any + Sync + Send>;

#[derive(Debug, Default)]
pub struct InstanceRegistry {
    instances: FxHashMap<TypeId, Instance>,
}

impl InstanceRegistry {
    pub async fn get<C: Any + Send + Sync>(&self) -> anyhow::Result<&Instance> {
        self.instances
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No instance found for type {:?}", type_name::<C>()))
    }

    pub fn set<C: Any + Send + Sync>(&mut self, instance: C) {
        self.instances.insert(TypeId::of::<C>(), Box::new(instance));
    }
}
