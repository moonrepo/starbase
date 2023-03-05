use crate::instance::InstanceRegistry;
use async_trait::async_trait;

#[derive(Debug, Default)]
pub struct Context {
    pub state: InstanceRegistry,
    pub resources: InstanceRegistry,
}

#[async_trait]
pub trait FromContext: Send + Sync + Sized {
    async fn from_context(context: &mut Context) -> anyhow::Result<Self>;
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
