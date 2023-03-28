use crate::context::Context;
use core::future::Future;
use futures::future::BoxFuture;
use std::fmt::Debug;

pub type SystemResult = anyhow::Result<()>;
pub type SystemFutureResult = BoxFuture<'static, SystemResult>;

pub trait System: Debug + Send + Sync {
    fn initialize(&mut self, _context: Context) -> SystemFutureResult {
        Box::pin(async { Ok(()) })
    }

    fn analyze(&mut self, _context: Context) -> SystemFutureResult {
        Box::pin(async { Ok(()) })
    }

    fn execute(&mut self, _context: Context) -> SystemFutureResult {
        Box::pin(async { Ok(()) })
    }

    fn finalize(&mut self, _context: Context) -> SystemFutureResult {
        Box::pin(async { Ok(()) })
    }
}

pub type BoxedSystem = Box<dyn System>;

pub trait SystemFunc: Send + Sync {
    fn call(self: Box<Self>, context: Context) -> SystemFutureResult;
}

impl<T: Send + Sync, F> SystemFunc for T
where
    T: FnOnce(Context) -> F,
    F: Future<Output = SystemResult> + Send + 'static,
{
    fn call(self: Box<Self>, context: Context) -> SystemFutureResult {
        Box::pin(self(context))
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

        impl System for $variant {
            fn $func(&mut self, context: Context) -> SystemFutureResult {
                let func = self.func.take().unwrap();
                func.call(context)
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
