use crate::context::{Context, FromContext};
use crate::system::{IntoSystem, System};
use futures::future::BoxFuture;
use tokio::sync::RwLock;

pub struct App {
    context: Context,
    initializers: Vec<System>,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::new(),
            initializers: Vec::new(),
        }
    }

    pub fn add_initializer(&mut self, system: impl IntoSystem) -> &mut Self {
        self.initializers.push(system.into_system());
        self
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        // dbg!(&self);

        for s in self.initializers.drain(..) {
            // s.execute().await.unwrap();
        }

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
}
