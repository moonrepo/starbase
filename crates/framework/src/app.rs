use tokio::task;

use crate::context::Context;
use crate::system::{BoxedSystem, System, SystemFutureResult};

pub struct App {
    context: Context,
    initializers: Vec<BoxedSystem>,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::default(),
            initializers: Vec::new(),
        }
    }

    pub fn add_initializer(&mut self, system: impl System + 'static) -> &mut Self {
        self.initializers.push(Box::new(system));
        self
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut context = std::mem::take(&mut self.context);
        let initializers = self.initializers.drain(..).collect::<Vec<_>>();

        self.execute_systems(&mut context, initializers).await?;

        Ok(())
    }

    // Private

    fn execute_systems(
        &mut self,
        context: &mut Context,
        systems: Vec<BoxedSystem>,
    ) -> SystemFutureResult {
        Box::pin(async move {
            let mut futures = vec![];

            for system in systems {
                futures.push(task::spawn(system.execute()));
            }

            futures::future::try_join_all(futures).await?;

            Ok(())
        })
    }
}
