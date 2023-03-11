use crate::instance::InstanceRegistry;
use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct ContextManager {
    state: Arc<InstanceRegistry>,
    pub resources: Arc<InstanceRegistry>,
}

impl ContextManager {
    pub async fn state<T: Any + Send + Sync>(&self) -> anyhow::Result<&T> {
        Ok(self.state.get::<T>()?.downcast_ref::<T>().unwrap())
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
