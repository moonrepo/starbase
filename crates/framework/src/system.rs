use crate::context::ActiveContext;
use core::future::Future;
use futures::future::BoxFuture;

pub type SystemResult = anyhow::Result<()>;
pub type SystemFutureResult = BoxFuture<'static, SystemResult>;

pub trait System: Send + Sync {
    fn execute(&self, context: ActiveContext) -> SystemFutureResult;
}

impl<T: Send + Sync, F> System for T
where
    T: Fn(ActiveContext) -> F,
    F: Future<Output = SystemResult> + Send + Sync + 'static,
{
    fn execute(&self, context: ActiveContext) -> SystemFutureResult {
        Box::pin(self(context))
    }
}

pub type BoxedSystem = Box<dyn System>;
