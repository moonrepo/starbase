use crate::app_state::*;
use crate::args::{ArgsMap, ExecuteArgs};
use crate::emitters::{EmitterInstance, EmitterManager, Emitters};
use crate::resources::{ResourceInstance, ResourceManager, Resources};
use crate::states::{StateInstance, StateManager, States};
use crate::system::{BoxedSystem, CallbackSystem, System, SystemFunc};
use crate::tracing::TracingOptions;
use miette::IntoDiagnostic;
use std::any::Any;
use std::mem;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task;
use tracing::{instrument, trace};

pub type AppResult<T = ()> = miette::Result<T>;
pub type MainResult = miette::Result<()>;

#[derive(Debug, Default)]
pub enum Phase {
    #[default]
    Startup,
    Analyze,
    Execute,
    Shutdown,
}

pub trait AppExtension {
    fn extend(self, app: &mut App) -> AppResult;
}

#[derive(Debug)]
pub struct App {
    // Data
    args: Arc<ArgsMap>,
    emitters: Emitters,
    resources: Resources,
    states: States,

    // Systems
    startups: Vec<BoxedSystem>,
    analyzers: Vec<BoxedSystem>,
    executors: Vec<BoxedSystem>,
    shutdowns: Vec<BoxedSystem>,
}

impl App {
    /// Create a new application instance.
    #[allow(clippy::new_without_default)]
    pub fn new() -> App {
        let mut app = App {
            analyzers: vec![],
            args: Arc::new(ArgsMap::default()),
            emitters: Arc::new(EmitterManager::default()),
            executors: vec![],
            shutdowns: vec![],
            startups: vec![],
            resources: Arc::new(ResourceManager::default()),
            states: Arc::new(StateManager::default()),
        };
        app.startup(start_startup_phase);
        app.analyze(start_analyze_phase);
        app.execute(start_execute_phase);
        app.shutdown(start_shutdown_phase);
        app
    }

    /// Setup `miette` diagnostics by registering error and panic hooks.
    pub fn setup_diagnostics() {
        crate::diagnostics::setup_miette();
    }

    /// Setup `tracing` messages with default options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing() -> crate::tracing::TracingGuard {
        Self::setup_tracing_with_options(TracingOptions::default())
    }

    /// Setup `tracing` messages with custom options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing_with_options(options: TracingOptions) -> crate::tracing::TracingGuard {
        crate::tracing::setup_tracing(options)
    }

    /// Extend the app with an extension that contains a set of systems.
    pub fn extend(&mut self, extension: impl AppExtension) -> AppResult<()> {
        extension.extend(self)
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

    /// Add a system function with the provided arguments that runs during the execute phase.
    pub fn execute_with_args<S: SystemFunc + 'static, A: Any + Send + Sync>(
        &mut self,
        system: S,
        args: A,
    ) -> &mut Self {
        self.set_args(args);
        self.execute(system);
        self
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

    /// Add an args instance to the application context.
    pub fn set_args<A: Any + Send + Sync>(&mut self, args: A) -> &mut Self {
        self.args.set_sync(args);
        self
    }

    /// Add an event emitter instance to the application context.
    pub fn set_emitter<M: Any + Send + Sync + EmitterInstance>(
        &mut self,
        instance: M,
    ) -> &mut Self {
        self.emitters.set_sync(instance);
        self
    }

    /// Add a resource instance to the application context.
    pub fn set_resource<R: Any + Send + Sync + ResourceInstance>(
        &mut self,
        instance: R,
    ) -> &mut Self {
        self.resources.set_sync(instance);
        self
    }

    /// Add a state instance to the application context.
    pub fn set_state<S: Any + Send + Sync + StateInstance>(&mut self, instance: S) -> &mut Self {
        self.states.set_sync(instance);
        self
    }

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(mut self) -> miette::Result<Arc<StateManager>> {
        let states = Arc::clone(&self.states);
        states.set_sync(ExecuteArgs(Arc::clone(&self.args)));

        let resources = Arc::clone(&self.resources);

        let emitters = Arc::clone(&self.emitters);

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

        Ok(states)
    }

    // Private

    #[instrument(skip_all)]
    async fn run_startup(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.startups);

        trace!("Running startup phase with {} systems", systems.len());

        self.run_systems_in_serial(systems, states, resources, emitters)
            .await?;

        trace!("Ran startup phase");

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_analyze(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.analyzers);

        trace!("Running analyze phase with {} systems", systems.len());

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        trace!("Ran analyze phase");

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_execute(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.executors);

        trace!("Running execute phase with {} systems", systems.len());

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        trace!("Ran execute phase");

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_shutdown(
        &mut self,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let systems = mem::take(&mut self.shutdowns);

        trace!("Running shutdown phase with {} systems", systems.len());

        self.run_systems_in_parallel(systems, states, resources, emitters)
            .await?;

        trace!("Ran shutdown phase");

        Ok(())
    }

    async fn run_systems_in_parallel(
        &self,
        systems: Vec<BoxedSystem>,
        states: States,
        resources: Resources,
        emitters: Emitters,
    ) -> AppResult {
        let mut futures = Vec::with_capacity(systems.len());
        let semaphore = Arc::new(Semaphore::new(num_cpus::get()));

        for system in systems {
            let permit = semaphore.clone().acquire_owned().await.into_diagnostic()?;
            let states = Arc::clone(&states);
            let resources = Arc::clone(&resources);
            let emitters = Arc::clone(&emitters);

            futures.push(task::spawn(async move {
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
