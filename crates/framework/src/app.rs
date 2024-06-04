use crate::session::{AppResult, AppSession};
use crate::tracing::TracingOptions;
use futures::try_join;
use std::future::Future;
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

#[derive(Debug, Default)]
pub struct App<S: AppSession> {
    pub phase: AppPhase,
    _marker: std::marker::PhantomData<S>,
}

impl<S: AppSession> App<S> {
    /// Setup `miette` diagnostics by registering error and panic hooks.
    pub fn setup_diagnostics(&self) {
        crate::diagnostics::setup_miette();
    }

    /// Setup `tracing` messages with default options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing(&self) -> crate::tracing::TracingGuard {
        self.setup_tracing_with_options(TracingOptions::default())
    }

    /// Setup `tracing` messages with custom options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing_with_options(
        &self,
        options: TracingOptions,
    ) -> crate::tracing::TracingGuard {
        crate::tracing::setup_tracing(options)
    }

    /// Start the application and run all registered systems grouped into phases.
    #[instrument(skip_all)]
    pub async fn run<F, Fut>(mut self, mut session: S, op: F) -> miette::Result<S>
    where
        F: FnOnce(&S) -> Fut,
        Fut: Future<Output = AppResult> + 'static,
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
        if let Err(error) = self.run_execute(&session, op).await {
            self.run_shutdown(&mut session).await?;

            return Err(error);
        }

        // Shutdown
        self.run_shutdown(&mut session).await?;

        Ok(session)
    }

    // Private

    #[instrument(skip_all)]
    async fn run_startup(&mut self, session: &mut S) -> AppResult {
        trace!("Running startup phase");

        self.phase = AppPhase::Startup;
        session.startup().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_analyze(&mut self, session: &mut S) -> AppResult {
        trace!("Running analyze phase");

        self.phase = AppPhase::Analyze;
        session.analyze().await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_execute<F, Fut>(&mut self, session: &S, op: F) -> AppResult
    where
        F: FnOnce(&S) -> Fut,
        Fut: Future<Output = AppResult> + 'static,
    {
        trace!("Running execute phase");

        self.phase = AppPhase::Execute;
        try_join!(session.execute(), op(session))?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_shutdown(&mut self, session: &mut S) -> AppResult {
        trace!("Running shutdown phase");

        self.phase = AppPhase::Shutdown;
        session.shutdown().await?;

        Ok(())
    }
}
