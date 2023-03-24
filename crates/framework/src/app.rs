use crate::context::{Context, ContextManager};
use crate::system::{
    AnalyzeSystem, BoxedSystem, ExecuteSystem, FinalizeSystem, InitializeSystem, System, SystemFunc,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

#[derive(Debug, Default)]
pub struct App {
    context: ContextManager,

    // Systems for all phases
    systems: Vec<BoxedSystem>,

    // Systems by phase
    initializers: Vec<BoxedSystem>,
    analyzers: Vec<BoxedSystem>,
    executors: Vec<BoxedSystem>,
    finalizers: Vec<BoxedSystem>,
}

impl App {
    pub fn add_initializer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.initializers
            .push(Box::new(InitializeSystem::new(system)));
        self
    }

    pub fn add_analyzer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.analyzers.push(Box::new(AnalyzeSystem::new(system)));
        self
    }

    pub fn add_executor<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.executors.push(Box::new(ExecuteSystem::new(system)));
        self
    }

    pub fn add_finalizer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.finalizers.push(Box::new(FinalizeSystem::new(system)));
        self
    }

    pub fn add_system<S: System + 'static>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let context = Arc::new(RwLock::new(std::mem::take(&mut self.context)));

        self.run_initializers(Arc::clone(&context)).await?;
        self.run_analyzers(Arc::clone(&context)).await?;
        self.run_executors(Arc::clone(&context)).await?;
        self.run_finalizers(Arc::clone(&context)).await?;

        Ok(())
    }

    // Private

    async fn run_initializers(&mut self, context: Context) -> anyhow::Result<()> {
        for mut system in self.initializers.drain(..) {
            system.initialize(Arc::clone(&context)).await?;
        }

        for system in &mut self.systems {
            system.initialize(Arc::clone(&context)).await?;
        }

        Ok(())
    }

    async fn run_analyzers(&mut self, context: Context) -> anyhow::Result<()> {
        for mut system in self.analyzers.drain(..) {
            system.analyze(Arc::clone(&context)).await?;
        }

        for system in &mut self.systems {
            system.analyze(Arc::clone(&context)).await?;
        }

        Ok(())
    }

    async fn run_executors(&mut self, context: Context) -> anyhow::Result<()> {
        let mut futures = vec![];

        for mut system in self.executors.drain(..) {
            futures.push(task::spawn(system.execute(Arc::clone(&context))));
        }

        for system in &mut self.systems {
            futures.push(task::spawn(system.execute(Arc::clone(&context))));
        }

        futures::future::try_join_all(futures).await?;

        Ok(())
    }

    async fn run_finalizers(&mut self, context: Context) -> anyhow::Result<()> {
        let mut futures = vec![];

        for mut system in self.finalizers.drain(..) {
            futures.push(task::spawn(system.finalize(Arc::clone(&context))));
        }

        for system in &mut self.systems {
            futures.push(task::spawn(system.finalize(Arc::clone(&context))));
        }

        futures::future::try_join_all(futures).await?;

        Ok(())
    }
}
