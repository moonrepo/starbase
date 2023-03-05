use crate::context::{Context, FromContext};
use async_trait::async_trait;
// use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Copy)]
pub struct State {
    // pub value: &'sys S,
}

#[async_trait]
impl FromContext for State {
    async fn from_context(_context: &mut Context) -> anyhow::Result<Self> {
        // let value = context.state.get::<S>().await?.read().await;

        // let value = value.downcast_ref::<S>().unwrap();

        Ok(Self {})
    }
}

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
