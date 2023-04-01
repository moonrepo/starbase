use crate::context::{Context, ContextManager};
use crate::events::EmitterInstance;
use crate::resource::ResourceInstance;
use crate::state::StateInstance;
use crate::system::{BoxedSystem, CallbackSystem, System, SystemFunc};
use crate::{EmitterManager, ResourceManager, StateManager};
use futures::future::try_join_all;
use std::any::Any;
use std::mem;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::task;

#[derive(Debug, Default)]
pub enum Phase {
    #[default]
    Initialize,
    Analyze,
    Execute,
    Finalize,
}

#[derive(Debug, Default)]
pub struct App {
    pub current_phase: Option<Phase>,

    // Data
    emitters: EmitterManager,
    resources: ResourceManager,
    states: StateManager,

    // Systems
    initializers: Vec<BoxedSystem>,
    analyzers: Vec<BoxedSystem>,
    executors: Vec<BoxedSystem>,
    finalizers: Vec<BoxedSystem>,
}

impl App {
    /// Add a system function that runs during the initialization phase.
    pub fn add_initializer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Initialize, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the analysis phase.
    pub fn add_analyzer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Analyze, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the execution phase.
    pub fn add_executor<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Execute, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the finalization phase.
    pub fn add_finalizer<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Finalize, CallbackSystem::new(system))
    }

    /// Add a system that runs during the specified phase.
    pub fn add_system<S: System + 'static>(&mut self, phase: Phase, system: S) -> &mut Self {
        match phase {
            Phase::Initialize => {
                self.initializers.push(Box::new(system));
            }
            Phase::Analyze => {
                self.analyzers.push(Box::new(system));
            }
            Phase::Execute => {
                self.executors.push(Box::new(system));
            }
            Phase::Finalize => {
                self.finalizers.push(Box::new(system));
            }
        };

        self
    }

    /// Add an event emitter instance to the application context.
    pub fn add_emitter<M: Any + Send + Sync + EmitterInstance>(
        &mut self,
        instance: M,
    ) -> &mut Self {
        self.emitters.set(instance);
        self
    }

    /// Add a resource instance to the application context.
    pub fn add_resource<R: Any + Send + Sync + ResourceInstance>(
        &mut self,
        instance: R,
    ) -> &mut Self {
        self.resources.set(instance);
        self
    }

    /// Add a state instance to the application context.
    pub fn add_state<S: Any + Send + Sync + StateInstance>(&mut self, instance: S) -> &mut Self {
        self.states.set(instance);
        self
    }

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(&mut self) -> anyhow::Result<ContextManager> {
        let context = Arc::new(RwLock::new(mem::take(&mut self.context)));

        // Initialize
        let initializers = mem::take(&mut self.initializers);

        self.current_phase = Some(Phase::Initialize);
        self.run_systems_in_serial(initializers, Arc::clone(&context))
            .await?;

        // Analyze
        let analyzers = mem::take(&mut self.analyzers);

        self.current_phase = Some(Phase::Analyze);
        self.run_systems_in_parallel(analyzers, Arc::clone(&context))
            .await?;

        // Execute
        let executors = mem::take(&mut self.executors);

        self.current_phase = Some(Phase::Execute);
        self.run_systems_in_parallel(executors, Arc::clone(&context))
            .await?;

        // Finalize
        let finalizers = mem::take(&mut self.finalizers);

        self.current_phase = Some(Phase::Finalize);
        self.run_systems_in_parallel(finalizers, Arc::clone(&context))
            .await?;

        let context = Arc::try_unwrap(context)
            .expect("Failed to acquire context before closing the application. This typically means that threads are still running that have not been awaited.")
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
        let semaphore = Arc::new(Semaphore::new(num_cpus::get()));

        for system in systems {
            let ctx = Arc::clone(&context);
            let permit = Arc::clone(&semaphore).acquire_owned().await?;

            futures.push(task::spawn(async move {
                let result = system.run(ctx).await;
                drop(permit);
                result
            }));
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
