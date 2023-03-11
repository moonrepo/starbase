use crate::context::Context;
use async_trait::async_trait;
use core::future::Future;
use futures::{future::BoxFuture, task::FutureObj};

// pub type FunctionSystem = dyn FnOnce(&mut Context) -> BoxFuture<'static, anyhow::Result<()>>;

pub type SystemResult = anyhow::Result<()>;
pub type SystemFutureResult = BoxFuture<'static, SystemResult>;

// pub type FunctionSystem =
//     dyn FnOnce(&mut Context) -> (dyn Future<Output = anyhow::Result<()>> + Send + Sync + 'static);

pub trait System: Send + Sync {
    fn execute(&self) -> SystemFutureResult;
}

pub type BoxedSystem = Box<dyn System>;

impl<T: Send + Sync, F> System for T
where
    T: Fn() -> F,
    F: Future<Output = SystemResult> + Send + Sync + 'static,
{
    fn execute(&self) -> SystemFutureResult {
        Box::pin(self())
    }
}

// pub struct SystemExecutor {
//     func: Box<FunctionSystem>,
// }

// pub trait IntoSystemExecutor {
//     fn into_system(self) -> SystemExecutor;
// }

// impl SystemExecutor {
//     pub async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
//         //(self.func)(context)?;
//         Ok(())
//     }
// }

// #[async_trait]
// impl<F, Fut> IntoSystemExecutor for F
// where
//     F: FnOnce(&mut Context) -> Fut,
//     Fut: Future<Output = anyhow::Result<()>> + Send + Sync + 'static,
// {
//     fn into_system(self) -> SystemExecutor {
//         SystemExecutor {
//             func: Box::new(self),
//         }
//     }
// }
