use crate::session::{AppResult, AppSession};
use crate::tracing::TracingOptions;
use miette::IntoDiagnostic;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{instrument, trace};

pub type MainResult = miette::Result<()>;

#[derive(Debug, Default)]
pub enum AppPhase {
    #[default]
    Startup,
    Analyze,
    Execute,
    Shutdown,
}

#[derive(Debug)]
pub struct App<S: AppSession> {
    pub phase: AppPhase,

    session: Option<S>,
}

impl<S: AppSession> App<S> {
    /// Create a new application instance.
    pub fn new(session: S) -> App<S> {
        App {
            phase: AppPhase::Startup,
            session: Some(session),
        }
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

    /// Start the application and run all registered systems grouped into phases.
    pub async fn run(mut self) -> miette::Result<S> {
        let mut session = self.session.take().unwrap();

        // Startup
        if let Err(error) = self.run_startup(&mut session).await {
            self.run_shutdown(&mut session).await?;

            return Err(error);
        }

        // Analyze
        if let Err(error) = self.run_analyze(&mut session).await {
            self.run_shutdown(&mut session).await?;

            return Err(error);
        }

        // Execute
        if let Err(error) = self.run_execute(&session).await {
            self.run_shutdown(&mut session).await?;

            return Err(error);
        }

        // Shutdown
        self.run_shutdown(&mut session).await?;

        Ok(session)
    }

    async fn run_in_parallel<Ses, I, F, Fut>(session: &Ses, systems: I) -> AppResult
    where
        Ses: AppSession,
        I: IntoIterator<Item = F>,
        F: FnOnce(&Ses) -> Fut,
        Fut: Future<Output = AppResult>,
    {
        let systems = systems.into_iter();
        let semaphore = Arc::new(Semaphore::new(num_cpus::get()));
        let mut futures = vec![];

        for system in systems {
            let _permit = semaphore.acquire().await.into_diagnostic()?;

            futures.push(system(session));
        }

        for future in futures {
            future.await?;
        }

        Ok(())
    }

    async fn run_in_serial<Ses, I, F, Fut>(session: &Ses, systems: I) -> AppResult
    where
        Ses: AppSession,
        I: IntoIterator<Item = F>,
        F: FnOnce(&Ses) -> Fut,
        Fut: Future<Output = AppResult>,
    {
        for system in systems.into_iter() {
            system(session).await?;
        }

        Ok(())
    }

    // Private

    #[instrument(skip_all)]
    async fn run_startup(&mut self, session: &mut S) -> AppResult {
        trace!("Running startup phase");

        session.startup().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_analyze(&mut self, session: &mut S) -> AppResult {
        trace!("Running analyze phase");

        session.analyze().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_execute(&mut self, session: &S) -> AppResult {
        trace!("Running execute phase");

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_shutdown(&mut self, session: &mut S) -> AppResult {
        trace!("Running shutdown phase");

        session.shutdown().await?;

        Ok(())
    }
}
