use crate::context::Context;
use async_trait::async_trait;
use core::future::Future;
use futures::future::BoxFuture;
use std::fmt::Debug;

pub type SystemResult = anyhow::Result<()>;
pub type SystemFutureResult = BoxFuture<'static, SystemResult>;

#[async_trait]
pub trait System: Debug + Send + Sync {
    async fn initialize(&self, _context: Context) -> SystemResult {
        Ok(())
    }

    async fn analyze(&self, _context: Context) -> SystemResult {
        Ok(())
    }

    async fn execute(&self, _context: Context) -> SystemResult {
        Ok(())
    }

    async fn finalize(&self, _context: Context) -> SystemResult {
        Ok(())
    }
}

pub type BoxedSystem = Box<dyn System>;

#[async_trait]
pub trait SystemFunc: Send + Sync {
    async fn call(self: Box<Self>, context: Context) -> SystemResult;
}

#[async_trait]
impl<T: Send + Sync, F> SystemFunc for T
where
    T: FnOnce(Context) -> F,
    F: Future<Output = SystemResult> + Send + 'static,
{
    async fn call(self: Box<Self>, context: Context) -> SystemResult {
        self(context).await
    }
}

macro_rules! system_variant_impl {
    ($variant:ident, $func:ident) => {
        pub struct $variant {
            pub func: Option<Box<dyn SystemFunc>>,
        }

        impl $variant {
            pub fn new<F: SystemFunc + 'static>(func: F) -> Self {
                Self {
                    func: Some(Box::new(func)),
                }
            }
        }

        #[async_trait]
        impl System for $variant {
            async fn $func(&self, context: Context) -> SystemResult {
                let func = self.func.take().unwrap();
                func.call(context).await
            }
        }

        impl Debug for $variant {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("$variant").finish()
            }
        }
    };
}

system_variant_impl!(InitializeSystem, initialize);
system_variant_impl!(AnalyzeSystem, analyze);
system_variant_impl!(ExecuteSystem, execute);
system_variant_impl!(FinalizeSystem, finalize);
