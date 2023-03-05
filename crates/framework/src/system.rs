use crate::context::{Context, FromContext};
use async_trait::async_trait;
use std::marker::PhantomData;

#[async_trait]
pub trait System<Params> {
    async fn execute(self, context: &mut Context) -> anyhow::Result<()>;
}

#[async_trait]
impl<F> System<()> for F
where
    F: Fn() + Send + Sync,
{
    async fn execute(self, _context: &mut Context) -> anyhow::Result<()> {
        (self)();
        Ok(())
    }
}

#[async_trait]
impl<F, T> System<(T,)> for F
where
    F: Fn(T) + Send + Sync,
    T: FromContext,
{
    async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
        (self)(T::from_context(context).await?);
        Ok(())
    }
}

#[async_trait]
impl<F, T1, T2> System<(T1, T2)> for F
where
    F: Fn(T1, T2) + Send + Sync,
    T1: FromContext,
    T2: FromContext,
{
    async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
        (self)(
            T1::from_context(context).await?,
            T2::from_context(context).await?,
        );
        Ok(())
    }
}
