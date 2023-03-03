use std::pin::Pin;

use async_trait::async_trait;
use futures::{future::BoxFuture, Future};

pub trait SystemHandler: Send + Sync + 'static {}

pub type SystemFunc = dyn FnOnce() -> anyhow::Result<()>;

pub struct System {
    func: Box<SystemFunc>,
}

impl System {
    pub fn new(func: Box<SystemFunc>) -> Self {
        System { func }
    }

    pub async fn execute(self) -> anyhow::Result<()> {
        (self.func)()
    }
}

pub trait IntoSystem {
    fn into_system(self) -> System;
}

impl<F> IntoSystem for F
where
    F: FnOnce() -> anyhow::Result<()> + 'static,
{
    fn into_system(self) -> System {
        System::new(Box::new(self))
    }
}
