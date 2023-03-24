use anyhow::anyhow;
use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type Instance = Box<dyn Any + Sync + Send>;

#[derive(Debug, Default)]
pub struct ContextManager {
    resources: FxHashMap<TypeId, Instance>,
    state: FxHashMap<TypeId, Instance>,
}

impl ContextManager {
    pub fn resource<C: Any + Send + Sync>(&self) -> anyhow::Result<&C> {
        let value = self
            .resources
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No resource found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_ref::<C>().unwrap();

        Ok(value)
    }

    pub fn resource_mut<C: Any + Send + Sync>(&mut self) -> anyhow::Result<&mut C> {
        let value = self
            .resources
            .get_mut(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No resource found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_mut::<C>().unwrap();

        Ok(value)
    }

    pub fn state<C: Any + Send + Sync>(&self) -> anyhow::Result<&C> {
        let value = self
            .state
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No state found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_ref::<C>().unwrap();

        Ok(value)
    }

    pub fn state_mut<C: Any + Send + Sync>(&mut self) -> anyhow::Result<&mut C> {
        let value = self
            .state
            .get_mut(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No state found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_mut::<C>().unwrap();

        Ok(value)
    }

    pub fn set_resource<C: Any + Send + Sync>(&mut self, instance: C) -> &mut Self {
        self.resources.insert(TypeId::of::<C>(), Box::new(instance));
        self
    }

    pub fn set_state<C: Any + Send + Sync>(&mut self, instance: C) -> &mut Self {
        self.state.insert(TypeId::of::<C>(), Box::new(instance));
        self
    }
}

pub type Context = Arc<RwLock<ContextManager>>;

#[async_trait]
pub trait FromContext: Send + Sync + Sized {
    async fn from_context(context: Context) -> anyhow::Result<Self>;
}
