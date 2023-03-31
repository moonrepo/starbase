use crate::context::{Context, ContextManager};
use crate::system::{
    AnalyzeSystem, BoxedSystem, ExecuteSystem, FinalizeSystem, InitializeSystem, System,
    SystemFunc, SystemResult,
};
use crate::SystemFutureResult;
use core::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

#[derive(Debug, Default)]
pub struct App {
    pub context: ContextManager,
    systems: Vec<BoxedSystem>,
}

impl App {
    /// Add a system function that runs during the initialization phase.
    pub fn add_initializer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(InitializeSystem::new(system)));
        self
    }

    /// Add a system function that runs during the analysis phase.
    pub fn add_analyzer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(AnalyzeSystem::new(system)));
        self
    }

    /// Add a system function that runs during the execution phase.
    pub fn add_executor<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(ExecuteSystem::new(system)));
        self
    }

    /// Add a system function that runs during the finalization phase.
    pub fn add_finalizer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(FinalizeSystem::new(system)));
        self
    }

    /// Add a system instance composed of methods mapping to phases,
    /// where each method will be ran during the corresponding phase.
    pub fn add_system<S: System + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(&mut self) -> anyhow::Result<ContextManager> {
        let context = Arc::new(RwLock::new(std::mem::take(&mut self.context)));
        let systems = std::mem::take(&mut self.systems);

        // Initialize
        self.run_systems_in_serial(&systems, Arc::clone(&context), |system, ctx| {
            system.initialize(ctx)
        })
        .await?;

        // Analyze
        self.run_systems_in_parallel(&systems, Arc::clone(&context), |system, ctx| {
            system.analyze(ctx)
        })
        .await?;

        // Execute
        self.run_systems_in_parallel(&systems, Arc::clone(&context), |system, ctx| {
            system.execute(ctx)
        })
        .await?;

        // Finalize
        self.run_systems_in_parallel(&systems, Arc::clone(&context), |system, ctx| {
            system.finalize(ctx)
        })
        .await?;

        self.systems.clear();

        let context = Arc::try_unwrap(context)
            .expect("Failed to gather context before closing the application. This typically means that threads are still running that have not been awaited.")
            .into_inner();

        Ok(context)
    }

    // Private

    async fn run_systems_in_parallel<'app, F>(
        &self,
        systems: &'app [BoxedSystem],
        context: Context,
        handler: F,
    ) -> anyhow::Result<()>
    where
        F: Fn(&'app BoxedSystem, Context) -> SystemFutureResult,
    {
        let mut futures = vec![];

        for system in systems {
            futures.push(task::spawn(handler(system, Arc::clone(&context))));
        }

        for future in futures {
            future.await??;
        }

        Ok(())
    }

    async fn run_systems_in_serial<'app, F, Fut>(
        &self,
        systems: &'app [BoxedSystem],
        context: Context,
        handler: F,
    ) -> anyhow::Result<()>
    where
        F: Fn(&'app BoxedSystem, Context) -> Fut,
        Fut: Future<Output = SystemResult>,
    {
        for system in systems {
            handler(system, Arc::clone(&context)).await?;
        }

        Ok(())
    }
}
