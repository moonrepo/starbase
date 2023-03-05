use crate::context::{Context, FromContext};
// use crate::system::{BoxedSystem, IntoSystem, System};
use crate::system::System;
use futures::future::BoxFuture;
use tokio::sync::RwLock;

pub struct App {
    context: Context,
    // initializers: Vec<BoxedSystem>,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::new(),
            //  initializers: Vec::new(),
        }
    }

    // pub fn add_initializer<P>(&mut self, system: impl IntoSystem) -> &mut Self {
    //     self.initializers.push(Box::new(system.into_system()));
    //     self
    // }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        // dbg!(&self);

        // for s in self.initializers.drain(..) {
        //     // s.execute().await.unwrap();
        // }

        Ok(())
    }

    // Private

    // async fn execute_system<'app, S, X>(
    //     &'app mut self,
    //     system: S,
    // ) -> BoxFuture<'app, Result<S, S::Error>>
    // where
    //     S: FromContext<X>,
    // {
    //     let ctx = &mut self.context;

    //     Box::pin(async move { S::from_context(ctx).await })
    // }

    async fn execute_system<S, P>(&mut self, system: S) -> anyhow::Result<()>
    where
        S: System<P>,
    {
        system.execute(&mut self.context).await?;

        Ok(())
    }
}
