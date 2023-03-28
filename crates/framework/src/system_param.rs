// use crate::context::{Context, FromContext};
// use async_trait::async_trait;

// A parameter that is passed to a `System` as a function argument,
// and can access context from the parent application.

// #[async_trait]
// impl FromContext for () {
//     type Value = ();

//     async fn from_context(_context: &mut Context) -> anyhow::Result<Self::Value> {
//         Ok(())
//     }
// }

// #[async_trait]
// impl<P0: FromContext> FromContext for (P0,) {
//     type Value = (P0::Value,);

//     async fn from_context(context: &mut Context) -> anyhow::Result<Self::Value> {
//         Ok((P0::from_context(context).await?,))
//     }
// }

// #[async_trait]
// impl<P0: FromContext, P1: FromContext> FromContext for (P0, P1) {
//     type Value = (P0::Value, P1::Value);

//     async fn from_context(context: &mut Context) -> anyhow::Result<Self::Value> {
//         Ok((
//             P0::from_context(context).await?,
//             P1::from_context(context).await?,
//         ))
//     }
// }
