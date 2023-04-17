use crate::app_state::*;
use crate::emitters::{EmitterInstance, EmitterManager, Emitters};
use crate::resources::{ResourceInstance, ResourceManager, Resources};
use crate::states::{StateInstance, StateManager, States};
use crate::system::{BoxedSystem, CallbackSystem, System, SystemFunc};
use crate::tracing::TracingOptions;
use miette::IntoDiagnostic;
use std::any::Any;
use std::mem;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::task;

pub type AppResult = miette::Result<()>;
pub type MainResult = miette::Result<()>;

#[derive(Debug, Default)]
pub enum Phase {
    #[default]
    Startup,
    Analyze,
    Execute,
    Shutdown,
}

#[derive(Debug)]
pub struct App {
    // Data
    emitters: EmitterManager,
    resources: ResourceManager,
    states: StateManager,

    // Systems
    startups: Vec<BoxedSystem>,
    analyzers: Vec<BoxedSystem>,
    executors: Vec<BoxedSystem>,
    shutdowns: Vec<BoxedSystem>,
}

impl App {
    #[allow(clippy::new_without_default)]
    pub fn new() -> App {
        let mut app = App {
            analyzers: vec![],
            emitters: EmitterManager::default(),
            executors: vec![],
            shutdowns: vec![],
            startups: vec![],
            resources: ResourceManager::default(),
            states: StateManager::default(),
        };
        app.startup(start_startup_phase);
        app.analyze(start_analyze_phase);
        app.execute(start_execute_phase);
        app.shutdown(start_shutdown_phase);
        app
    }

    #[cfg(feature = "tracing")]
    pub fn setup_tracing() {
        Self::setup_tracing_with_options(TracingOptions::default())
    }

    #[cfg(feature = "tracing")]
    pub fn setup_tracing_with_options(options: TracingOptions) {
        crate::tracing::set_tracing_subscriber(options);
    }

    pub fn setup_hooks() {
        crate::diagnostic::set_miette_hooks();
    }

    /// Add a system function that runs during the startup phase.
    pub fn startup<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Startup, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the analyze phase.
    pub fn analyze<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Analyze, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the execute phase.
    pub fn execute<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Execute, CallbackSystem::new(system))
    }

    /// Add a system function that runs during the shutdown phase.
    pub fn shutdown<S: SystemFunc + 'static>(&mut self, system: S) -> &mut Self {
        self.add_system(Phase::Shutdown, CallbackSystem::new(system))
    }

    /// Add a system that runs during the specified phase.
    pub fn add_system<S: System + 'static>(&mut self, phase: Phase, system: S) -> &mut Self {
        match phase {
            Phase::Startup => {
                self.startups.push(Box::new(system));
            }
            Phase::Analyze => {
                self.analyzers.push(Box::new(system));
            }
            Phase::Execute => {
                self.executors.push(Box::new(system));
            }
            Phase::Shutdown => {
                self.shutdowns.push(Box::new(system));
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
    pub async fn run(&mut self) -> miette::Result<StateManager> {
        let emitters = Arc::new(RwLock::new(mem::take(&mut self.emitters)));
        let resources = Arc::new(RwLock::new(mem::take(&mut self.resources)));
        let states = Arc::new(RwLock::new(mem::take(&mut self.states)));

        // Startup
        if let Err(error) = self
            .run_startup(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_shutdown(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Analyze
        if let Err(error) = self
            .run_analyze(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_shutdown(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Execute
        if let Err(error) = self
            .run_execute(states.clone(), resources.clone(), emitters.clone())
            .await
        {
            self.run_shutdown(states.clone(), resources.clone(), emitters.clone())
                .await?;

            return Err(error);
        }

        // Shutdown
        self.run_shutdown(states.clone(), resources.clone(), emitters.clone())
            .await?;

        let states = Arc::try_unwrap(states)
            .expect("Failed to acquire state before closing the application. This typically means that threads are still running that have not been awaited.")
            .into_inner();

        Ok(states)
    }

    // Private

    async fn run_startup(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.startups);

        self.run_systems_in_serial(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_analyze(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.analyzers);

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_execute(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.executors);

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        Ok(())
    }

    async fn run_shutdown(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.shutdowns);

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
    ) -> AppResult {
        let mut futures = vec![];
        let semaphore = Arc::new(Semaphore::new(num_cpus::get()));

        for system in systems {
            let states = Arc::clone(&states);
            let resources = Arc::clone(&resources);
            let emitters = Arc::clone(&emitters);
            let semaphore = Arc::clone(&semaphore);

            futures.push(task::spawn(async move {
                let permit = semaphore.acquire().await.into_diagnostic()?;
                let result = system.run(states, resources, emitters).await;
                drop(permit);
                result
            }));
        }

        for future in futures {
            future.await.into_diagnostic()??;
        }

        Ok(())
    }

    async fn run_systems_in_serial(
        &self,
        systems: Vec<BoxedSystem>,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        for system in systems {
            let states = Arc::clone(&states);
            let resources = Arc::clone(&resources);
            let emitters = Arc::clone(&emitters);

            system.run(states, resources, emitters).await?;
        }

        Ok(())
    }
}
