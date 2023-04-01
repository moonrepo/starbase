use crate::context::Context;
use async_trait::async_trait;
use core::future::Future;
use std::fmt::Debug;

pub type SystemResult = anyhow::Result<()>;

#[async_trait]
pub trait System: Debug + Send + Sync {
    async fn run(&self, context: Context) -> SystemResult;
}

pub type BoxedSystem = Box<dyn System>;

#[async_trait]
pub trait SystemFunc: Send + Sync {
    async fn call(&self, context: Context) -> SystemResult;
}

#[async_trait]
impl<T: Send + Sync, F> SystemFunc for T
where
    T: Fn(Context) -> F,
    F: Future<Output = SystemResult> + Send + 'static,
{
    async fn call(&self, context: Context) -> SystemResult {
        self(context).await
    }
}

pub struct CallbackSystem {
    func: Box<dyn SystemFunc>,
}

impl CallbackSystem {
    pub fn new<F: SystemFunc + 'static>(func: F) -> Self {
        Self {
            func: Box::new(func),
        }
    }
}

#[async_trait]
impl System for CallbackSystem {
    async fn run(&self, context: Context) -> SystemResult {
        self.func.call(context).await
    }
}

impl Debug for CallbackSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackSystem").finish()
    }
}
