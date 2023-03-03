use std::marker::PhantomData;

use crate::context::{Context, FromContext};

pub type SystemResult = anyhow::Result<()>;

pub trait SystemHandler: Send + Sync + 'static {}

pub type SystemFunc = dyn FnOnce() -> anyhow::Result<()>;

pub struct System {
    func: Box<SystemFunc>,
}

impl System {
    pub fn new(func: Box<SystemFunc>) -> Self {
        System { func }
    }

    pub async fn execute(self, context: &mut Context) -> anyhow::Result<()> {
        (self.func)()
    }
}

pub trait IntoSystem {
    fn into_system(self) -> System;
}

// impl<F> IntoSystem for F
// where
//     F: FnOnce() -> anyhow::Result<()> + 'static,
// {
//     fn into_system(self) -> System {
//         System::new(Box::new(self))
//     }
// }

// pub struct System2<Params> {
//     marker: PhantomData<fn() -> Params>,
// }

// pub trait IntoSystem2<Marker>: Send + Sync + 'static {
//     type Params;

//     fn into_system(self) -> System2<Self::Params>;
// }

// impl<Func: Send + Sync + 'static, F0: FromContext> IntoSystem2<fn(F0) -> ()> for Func
// where
//     Func: FnMut() -> SystemResult,
// {
//     type Params = (F0);

//     fn into_system(self) -> System2<Self::Params> {
//         System2::new(Box::new(self))
//     }
// }
