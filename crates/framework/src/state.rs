use crate::context::{Context, FromContext};
use async_trait::async_trait;
use std::{
    convert::Infallible,
    ops::{Deref, DerefMut},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct State<S> {
    pub value: S,
}

#[async_trait]
impl<OuterState, InnerState> FromContext<OuterState> for State<InnerState>
where
    OuterState: Send + Sync,
{
    type Error = Infallible;

    async fn from_context(_context: &mut Context) -> Result<Self, Self::Error> {
        // let inner_state = InnerState::from_ref(state);
        Ok(Self { value })
    }
}

impl<S> Deref for State<S> {
    type Target = S;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<S> DerefMut for State<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}
