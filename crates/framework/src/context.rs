use crate::instance::InstanceRegistry;
use async_trait::async_trait;
use std::convert::Infallible;

#[derive(Debug)]
pub struct Context {
    state: InstanceRegistry,
    resources: InstanceRegistry,
}

impl Context {
    pub fn new() -> Self {
        Context {
            state: InstanceRegistry::new(),
            resources: InstanceRegistry::new(),
        }
    }
}

#[async_trait]
pub trait FromContext: Send + Sync + Sized {
    type Value: Send + Sync;

    async fn from_context(context: &mut Context) -> anyhow::Result<Self::Value>;
}

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
