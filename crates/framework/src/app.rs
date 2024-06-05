use crate::session::{AppResult, AppSession};
use crate::tracing::TracingOptions;
use futures::try_join;
use std::future::Future;
use tracing::{instrument, trace};

pub type MainResult = miette::Result<()>;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum AppPhase {
    #[default]
    Startup,
    Analyze,
    Execute,
    Shutdown,
}

#[derive(Debug, Default)]
pub struct App {
    pub phase: AppPhase,
}

impl App {
    /// Setup `miette` diagnostics by registering error and panic hooks.
    pub fn setup_diagnostics(&self) {
        crate::diagnostics::setup_miette();
    }

    /// Setup `tracing` messages with default options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing_with_defaults(&self) -> crate::tracing::TracingGuard {
        self.setup_tracing(TracingOptions::default())
    }

    /// Setup `tracing` messages with custom options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing(&self, options: TracingOptions) -> crate::tracing::TracingGuard {
        crate::tracing::setup_tracing(options)
    }

    /// Start the application with the provided session and execute all phases
    /// in order. If a phase fails, always run the shutdown phase.
    #[instrument(skip_all)]
    pub async fn run<S, F, Fut>(mut self, mut session: S, op: F) -> miette::Result<S>
    where
        S: AppSession,
        F: FnOnce(S) -> Fut,
        Fut: Future<Output = AppResult>,
    {
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
        if let Err(error) = self.run_execute(&mut session, op).await {
            self.run_shutdown(&mut session).await?;

            return Err(error);
        }

        // Shutdown
        self.run_shutdown(&mut session).await?;

        Ok(session)
    }

    // Private

    #[instrument(skip_all)]
    async fn run_startup<S>(&mut self, session: &mut S) -> AppResult
    where
        S: AppSession,
    {
        trace!("Running startup phase");

        self.phase = AppPhase::Startup;
        session.startup().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_analyze<S>(&mut self, session: &mut S) -> AppResult
    where
        S: AppSession,
    {
        trace!("Running analyze phase");

        self.phase = AppPhase::Analyze;
        session.analyze().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_execute<S, F, Fut>(&mut self, session: &mut S, op: F) -> AppResult
    where
        S: AppSession,
        F: FnOnce(S) -> Fut,
        Fut: Future<Output = AppResult>,
    {
        trace!("Running execute phase");

        self.phase = AppPhase::Execute;

        let execute_session = session.clone();

        try_join!(session.execute(), op(execute_session))?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_shutdown<S>(&mut self, session: &mut S) -> AppResult
    where
        S: AppSession,
    {
        trace!("Running shutdown phase");

        self.phase = AppPhase::Shutdown;
        session.shutdown().await?;

        Ok(())
    }
}
