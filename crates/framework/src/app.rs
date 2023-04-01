use crate::context::{Context, ContextManager};
use crate::system::{BoxedSystem, CallbackSystem, SystemFunc};
use futures::future::try_join_all;
use std::mem;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

#[derive(Debug, Default)]
pub struct App {
    pub context: ContextManager,

    // Systems
    initializers: Vec<BoxedSystem>,
    analyzers: Vec<BoxedSystem>,
    executors: Vec<BoxedSystem>,
    finalizers: Vec<BoxedSystem>,
}

impl App {
    /// Add a system function that runs during the initialization phase.
    pub fn add_initializer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.initializers
            .push(Box::new(CallbackSystem::new(system)));
        self
    }

    /// Add a system function that runs during the analysis phase.
    pub fn add_analyzer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.analyzers.push(Box::new(CallbackSystem::new(system)));
        self
    }

    /// Add a system function that runs during the execution phase.
    pub fn add_executor<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.executors.push(Box::new(CallbackSystem::new(system)));
        self
    }

    /// Add a system function that runs during the finalization phase.
    pub fn add_finalizer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.finalizers.push(Box::new(CallbackSystem::new(system)));
        self
    }

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(&mut self) -> anyhow::Result<ContextManager> {
        let context = Arc::new(RwLock::new(mem::take(&mut self.context)));

        // Initialize
        let initializers = mem::take(&mut self.initializers);

        self.run_systems_in_serial(initializers, Arc::clone(&context))
            .await?;

        // Analyze
        let analyzers = mem::take(&mut self.analyzers);

        self.run_systems_in_parallel(analyzers, Arc::clone(&context))
            .await?;

        // Execute
        let executors = mem::take(&mut self.executors);

        self.run_systems_in_parallel(executors, Arc::clone(&context))
            .await?;

        // Finalize
        let finalizers = mem::take(&mut self.finalizers);

        self.run_systems_in_parallel(finalizers, Arc::clone(&context))
            .await?;

        let context = Arc::try_unwrap(context)
            .expect("Failed to gather context before closing the application. This typically means that threads are still running that have not been awaited.")
            .into_inner();

        Ok(context)
    }

    // Private

    async fn run_systems_in_parallel(
        &self,
        systems: Vec<BoxedSystem>,
        context: Context,
    ) -> anyhow::Result<()> {
        let mut futures = vec![];

        for system in systems {
            let ctx = Arc::clone(&context);

            futures.push(task::spawn(async move { system.run(ctx).await }));
        }

        try_join_all(futures).await?;

        Ok(())
    }

    async fn run_systems_in_serial(
        &self,
        systems: Vec<BoxedSystem>,
        context: Context,
    ) -> anyhow::Result<()> {
        for system in systems {
            system.run(Arc::clone(&context)).await?;
        }

        Ok(())
    }
}
