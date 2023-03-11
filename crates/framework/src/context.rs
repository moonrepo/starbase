use crate::instance::InstanceRegistry;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Default)]
pub struct Context {
    pub state: InstanceRegistry,
    pub resources: InstanceRegistry,
}

pub struct ContextGuard<'outer, T: Send + Sync> {
    inner: &'outer T,
}

pub type ActiveContext = Arc<RwLock<Context>>;

#[async_trait]
pub trait FromContext: Send + Sync + Sized {
    async fn from_context(context: ActiveContext) -> anyhow::Result<Self>;
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
