// use crate::context::{ContextGuard, FromContext, SystemContext};
// use async_trait::async_trait;
// // use std::ops::{Deref, DerefMut};

// #[derive(Debug, Clone, Copy)]
// pub struct State<'sys, T: Send + Sync> {
//     pub value: &'sys T,
// }

// #[async_trait]
// impl<'sys, S: Send + Sync> FromContext<'static> for State<'sys, S> {
//     async fn from_context(context: ActiveContext) -> anyhow::Result<ContextGuard<'sys, Self>> {
//         let context = context.read().await;

//         let value = context.state.get::<S>()?;
//         let value = value.downcast_ref::<S>().unwrap();
//         let state = State { value };

//         Ok(ContextGuard { inner: &state })
//     }
// }

// impl<S> Deref for State<S> {
//     type Target = S;

//     fn deref(&self) -> &Self::Target {
//         &self.value
//     }
// }

// impl<S> DerefMut for State<S> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.value
//     }
// }

// #[async_trait]
// impl<F> System<()> for F
// where
//     F: Fn() + Send + Sync,
// {
//     async fn execute(self, _context: &mut Context) -> anyhow::Result<()> {
//         (self)();
//         Ok(())
//     }
// }

// #[async_trait]
// impl<F, T> System<(T,)> for F
// where
//     F: Fn(T) + Send + Sync,
//     T: FromContext,
// {
//     async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
//         (self)(T::from_context(context).await?);
//         Ok(())
//     }
// }

// #[async_trait]
// impl<F, T1, T2> System<(T1, T2)> for F
// where
//     F: Fn(T1, T2) + Send + Sync,
//     T1: FromContext,
//     T2: FromContext,
// {
//     async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
//         (self)(
//             T1::from_context(context).await?,
//             T2::from_context(context).await?,
//         );
//         Ok(())
//     }
// }

// pub struct BoxedSystem<P>(Box<dyn System<P>>);

// impl<S> BoxedSystem<S>
// where
//     S: Send + Sync + 'static,
// {
//     async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
//         self.0.execute(context).await?;
//         Ok(())
//     }
// }
