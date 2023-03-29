use crate::events::{Emitter, Event};
use crate::resource::Resource;
use crate::state::State;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};
use std::sync::Arc;
use tokio::sync::RwLock;

pub type Instance = Box<dyn Any + Sync + Send>;

#[derive(Debug, Default)]
pub struct ContextManager {
    emitters: FxHashMap<TypeId, Instance>,
    resources: FxHashMap<TypeId, Instance>,
    state: FxHashMap<TypeId, Instance>,
}

impl ContextManager {
    /// Convenience method for emitting the provided event with a registered emitter.
    pub async fn emit<E: Event + 'static>(
        &mut self,
        event: E,
    ) -> anyhow::Result<(E, Option<E::Value>)> {
        self.emitter_mut::<E>().emit(event).await
    }

    pub fn emitter_mut<E: Event + 'static>(&mut self) -> &mut Emitter<E> {
        if let Some(value) = self.emitters.get_mut(&TypeId::of::<Emitter<E>>()) {
            return value.downcast_mut::<Emitter<E>>().unwrap();
        }

        panic!("No emitter found for type {:?}", type_name::<Emitter<E>>())
    }

    pub fn resource<C: Any + Send + Sync + Resource>(&self) -> &C {
        if let Some(value) = self.resources.get(&TypeId::of::<C>()) {
            return value.downcast_ref::<C>().unwrap();
        }

        panic!("No resource found for type {:?}", type_name::<C>())
    }

    pub fn resource_mut<C: Any + Send + Sync + Resource>(&mut self) -> &mut C {
        if let Some(value) = self.resources.get_mut(&TypeId::of::<C>()) {
            return value.downcast_mut::<C>().unwrap();
        }

        panic!("No resource found for type {:?}", type_name::<C>())
    }

    pub fn state<C: Any + Send + Sync + State>(&self) -> &C {
        if let Some(value) = self.state.get(&TypeId::of::<C>()) {
            return value.downcast_ref::<C>().unwrap();
        }

        panic!("No state found for type {:?}", type_name::<C>())
    }

    pub fn state_mut<C: Any + Send + Sync + State>(&mut self) -> &mut C {
        if let Some(value) = self.state.get_mut(&TypeId::of::<C>()) {
            return value.downcast_mut::<C>().unwrap();
        }

        panic!("No state found for type {:?}", type_name::<C>())
    }

    pub fn add_emitter<C: Any + Send + Sync>(&mut self, instance: C) -> &mut Self {
        self.emitters.insert(TypeId::of::<C>(), Box::new(instance));
        self
    }

    pub fn add_resource<C: Any + Send + Sync + Resource>(&mut self, instance: C) -> &mut Self {
        self.resources.insert(TypeId::of::<C>(), Box::new(instance));
        self
    }

    pub fn add_state<C: Any + Send + Sync + State>(&mut self, instance: C) -> &mut Self {
        self.state.insert(TypeId::of::<C>(), Box::new(instance));
        self
    }
}

pub type Context = Arc<RwLock<ContextManager>>;
