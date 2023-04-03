use crate::events::{EmitterInstance, EmitterManager, Emitters};
use crate::resources::{ResourceInstance, ResourceManager, Resources};
use crate::states::{StateInstance, StateManager, States};
use crate::system::{BoxedSystem, CallbackSystem, System, SystemFunc};
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
    pub fn set_emitter<M: Any + Send + Sync + EmitterInstance>(
        &mut self,
        instance: M,
    ) -> &mut Self {
        self.emitters.set(instance);
        self
    }

    /// Add a resource instance to the application context.
    pub fn set_resource<R: Any + Send + Sync + ResourceInstance>(
        &mut self,
        instance: R,
    ) -> &mut Self {
        self.resources.set(instance);
        self
    }

    /// Add a state instance to the application context.
    pub fn set_state<S: Any + Send + Sync + StateInstance>(&mut self, instance: S) -> &mut Self {
        self.states.set(instance);
        self
    }

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(&mut self) -> anyhow::Result<StateManager> {
        let emitters = Arc::new(RwLock::new(mem::take(&mut self.emitters)));
        let resources = Arc::new(RwLock::new(mem::take(&mut self.resources)));
        let states = Arc::new(RwLock::new(mem::take(&mut self.states)));

        // Initialize
        if let Err(error) = self
            .run_initializers(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_finalizers(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Analyze
        if let Err(error) = self
            .run_analyzers(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_finalizers(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Execute
        if let Err(error) = self
            .run_executors(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_finalizers(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Finalize
        self.run_finalizers(states.clone(), resources.clone(), emitters.clone())
            .await?;

        let states = Arc::try_unwrap(states)
            .expect("Failed to acquire state before closing the application. This typically means that threads are still running that have not been awaited.")
            .into_inner();

        Ok(states)
    }

    // Private

    async fn run_initializers(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        let systems = mem::take(&mut self.initializers);

        self.current_phase = Some(Phase::Initialize);

        self.run_systems_in_serial(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_analyzers(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        let systems = mem::take(&mut self.analyzers);

        self.current_phase = Some(Phase::Analyze);

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_executors(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        let systems = mem::take(&mut self.executors);

        self.current_phase = Some(Phase::Execute);

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_finalizers(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        let systems = mem::take(&mut self.finalizers);

        self.current_phase = Some(Phase::Finalize);

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_systems_in_parallel(
        &self,
        systems: Vec<BoxedSystem>,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        let mut futures = vec![];
        let semaphore = Arc::new(Semaphore::new(num_cpus::get()));

        for system in systems {
            let states = Arc::clone(&states);
            let resources = Arc::clone(&resources);
            let emitters = Arc::clone(&emitters);
            let permit = Arc::clone(&semaphore).acquire_owned().await?;

            futures.push(task::spawn(async move {
                let result = system.run(states, resources, emitters).await;
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
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> anyhow::Result<()> {
        for system in systems {
            let states = Arc::clone(&states);
            let resources = Arc::clone(&resources);
            let emitters = Arc::clone(&emitters);

            system.run(states, resources, emitters).await?;
        }

        Ok(())
    }
}
