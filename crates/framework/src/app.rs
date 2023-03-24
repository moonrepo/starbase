use crate::context::{Context, ContextManager};
use crate::system::{BoxedSystem, System, SystemFutureResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

pub struct App {
    context: ContextManager,
    initializers: Vec<BoxedSystem>,
}

impl App {
    pub fn new() -> Self {
        App {
            context: ContextManager::default(),
            initializers: Vec::new(),
        }
    }

    pub fn add_initializer<S: System + 'static>(&mut self, system: S) -> &mut Self {
        self.initializers.push(Box::new(system));
        self
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let context = Arc::new(RwLock::new(std::mem::take(&mut self.context)));
        let initializers = self.initializers.drain(..).collect::<Vec<_>>();

        self.execute_systems(Arc::clone(&context), initializers, false)
            .await?;

        Ok(())
    }

    // Private

    fn execute_systems(
        &mut self,
        context: Context,
        systems: Vec<BoxedSystem>,
        parallel: bool,
    ) -> SystemFutureResult {
        Box::pin(async move {
            if parallel {
                let mut futures = vec![];

                for system in systems {
                    futures.push(task::spawn(system.execute(Arc::clone(&context))));
                }

                futures::future::try_join_all(futures).await?;
            } else {
                for system in systems {
                    system.execute(Arc::clone(&context)).await?;
                }
            }

            Ok(())
        })
    }
}
