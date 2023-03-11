use crate::context::Context;
use core::future::Future;
use futures::future::BoxFuture;

pub type SystemResult = anyhow::Result<()>;
pub type SystemFutureResult = BoxFuture<'static, SystemResult>;

pub trait System: Send + Sync {
    fn execute(&self, context: Context) -> SystemFutureResult;
}

impl<T: Send + Sync, F> System for T
where
    T: Fn(Context) -> F,
    F: Future<Output = SystemResult> + Send + Sync + 'static,
{
    fn execute(&self, context: Context) -> SystemFutureResult {
        Box::pin(self(context))
    }
}

// impl<T: Send + Sync, F> System for T
// where
//     T: Fn(SystemContext) -> F,
//     F: Future<Output = SystemResult> + Send + Sync + 'static,
// {
//     fn execute(&self, context: SystemContext) -> SystemFutureResult {
//         Box::pin(self(context))
//     }
// }

pub type BoxedSystem = Box<dyn System>;
