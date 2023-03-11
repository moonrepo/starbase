use anyhow::anyhow;
use async_trait::async_trait;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};
use std::sync::{Arc, RwLock};

pub type Instance = Box<dyn Any + Sync + Send>;

#[derive(Debug, Default)]
pub struct ContextManager {
    resources: RwLock<FxHashMap<TypeId, Arc<Instance>>>,
    state: RwLock<FxHashMap<TypeId, Arc<Instance>>>,
}

impl ContextManager {
    pub fn resource<C: Any + Send + Sync>(&self) -> anyhow::Result<Arc<C>> {
        let data = self.resources.read().expect("Resources lock is poisoned!");

        let value = data
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No resource found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_ref::<Arc<C>>().unwrap();

        Ok(Arc::clone(value))
    }

    pub fn state<C: Any + Send + Sync>(&self) -> anyhow::Result<Arc<C>> {
        let data = self.state.read().expect("State lock is poisoned!");

        let value = data
            .get(&TypeId::of::<C>())
            .ok_or_else(|| anyhow!("No state found for type {:?}", type_name::<C>()))?;

        let value = value.downcast_ref::<Arc<C>>().unwrap();

        Ok(Arc::clone(value))
    }

    pub fn set_state<C: Any + Send + Sync>(&mut self, instance: C) {
        self.state
            .write()
            .expect("State lock is poisoned!")
            .insert(TypeId::of::<C>(), Arc::new(Box::new(instance)));
    }
}

pub type Context = Arc<ContextManager>;

#[async_trait]
pub trait FromContext: Send + Sync + Sized {
    async fn from_context(context: Context) -> anyhow::Result<Self>;
}

// #[async_trait]
// pub trait FromContext<'outer>: Send + Sync + Sized {
//     async fn from_context(context: ActiveContext) -> anyhow::Result<ContextGuard<'outer, Self>>;
// }

// #[async_trait]
// impl<S, T> FromContext<S> for Option<T>
// where
//     T: FromContext<S>,
//     S: Send + Sync,
// {
//     type Error = Infallible;

//     async fn from_context(context: &mut Context) -> Result<Option<T>, Self::Error> {
//         Ok(T::from_context(context).await.ok())
//     }
// }

// #[async_trait]
// impl<S, T> FromContext<S> for Result<T, T::Error>
// where
//     T: FromContext<S>,
//     S: Send + Sync,
// {
//     type Error = Infallible;

//     async fn from_context(context: &mut Context) -> Result<Self, Self::Error> {
//         Ok(T::from_context(context).await)
//     }
// }

pub struct ContextGuard<'a, T> {
    inner: &'a T,
}
