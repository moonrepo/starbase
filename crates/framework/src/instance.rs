use anyhow::anyhow;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::{
    any::{type_name, Any, TypeId},
    sync::RwLock,
};

pub type Instance = Box<dyn Any + Sync + Send>;

#[derive(Debug, Default)]
pub struct InstanceRegistry {
    instances: RwLock<FxHashMap<TypeId, Arc<Instance>>>,
}

impl InstanceRegistry {
    pub fn get<C: Any + Send + Sync>(&self) -> anyhow::Result<Arc<C>> {
        let data = self.instances.read().expect("Instances lock is poisoned!");

        let value = data
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No instance found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_ref::<Arc<C>>().unwrap();

        Ok(Arc::clone(value))
    }

    pub fn set<C: Any + Send + Sync>(&mut self, instance: C) {
        self.instances
            .write()
            .expect("Instances lock is poisoned!")
            .insert(TypeId::of::<C>(), Arc::new(Box::new(instance)));
    }
}
