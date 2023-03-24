use std::marker::PhantomData;

use crate::context::{Context, FromContext};

pub type SystemResult = anyhow::Result<()>;

pub trait SystemHandler: Send + Sync + 'static {}

pub type SystemFunc<Params> = dyn FnOnce(Params) -> anyhow::Result<()>;

pub struct System<Params> {
    func: Box<SystemFunc<Params>>,
}

impl<Params> System<Params> {
    pub fn new(func: Box<SystemFunc<Params>>) -> Self {
        System { func }
    }

    pub async fn execute(self, context: &mut Context, params: Params) -> anyhow::Result<()> {
        (self.func)(params)
    }
}

pub type BoxedSystem<Params = ()> = Box<System<Params>>;

pub trait IntoSystem {
    fn into_system<Params>(self) -> System<Params>;
}

impl<F> IntoSystem for F
where
    F: FnOnce() -> anyhow::Result<()> + 'static,
{
    fn into_system<Params>(self) -> System<Params> {
        System::new(Box::new(self))
    }
}

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
