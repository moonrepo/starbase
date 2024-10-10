use crate::session::{AppResult, AppSession};
use crate::tracing::TracingOptions;
use miette::IntoDiagnostic;
use std::future::Future;
use tokio::spawn;
use tokio::task::JoinHandle;
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
    pub exit_code: Option<u8>,
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
    pub async fn run<S, F, Fut>(self, mut session: S, op: F) -> miette::Result<u8>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult> + Send + 'static,
    {
        self.run_with_session(&mut session, op).await
    }

    /// Start the application with the provided session and execute all phases
    /// in order. If a phase fails, always run the shutdown phase.
    ///
    /// This method is similar to [`App#run`](#method.run) but doesn't consume
    /// the session, and instead accepts a mutable reference.
    #[instrument(skip_all)]
    pub async fn run_with_session<S, F, Fut>(mut self, session: &mut S, op: F) -> miette::Result<u8>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult> + Send + 'static,
    {
        // Startup
        if let Err(error) = self.run_startup(session).await {
            self.run_shutdown(session, true).await?;

            return Err(error);
        }

        // Analyze
        if let Err(error) = self.run_analyze(session).await {
            self.run_shutdown(session, true).await?;

            return Err(error);
        }

        // Execute
        if let Err(error) = self.run_execute(session, op).await {
            self.run_shutdown(session, true).await?;

            return Err(error);
        }

        // Shutdown
        self.run_shutdown(session, false).await?;

        Ok(self.exit_code.unwrap_or_default())
    }

    // Private

    #[instrument(skip_all)]
    async fn run_startup<S>(&mut self, session: &mut S) -> miette::Result<()>
    where
        S: AppSession,
    {
        trace!("Running startup phase");

        self.phase = AppPhase::Startup;
        self.handle_exit_code(session.startup().await?);

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_analyze<S>(&mut self, session: &mut S) -> miette::Result<()>
    where
        S: AppSession,
    {
        trace!("Running analyze phase");

        self.phase = AppPhase::Analyze;
        self.handle_exit_code(session.analyze().await?);

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_execute<S, F, Fut>(&mut self, session: &mut S, op: F) -> miette::Result<()>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult> + Send + 'static,
    {
        trace!("Running execute phase");

        self.phase = AppPhase::Execute;

        let fg_session = session.clone();
        let mut bg_session = session.clone();
        let mut futures: Vec<JoinHandle<AppResult>> = vec![];

        futures.push(spawn(async move { op(fg_session).await }));
        futures.push(spawn(async move { bg_session.execute().await }));

        for (index, future) in futures.into_iter().enumerate() {
            let code = future.await.into_diagnostic()??;

            if index == 0 {
                self.handle_exit_code(code);
            }
        }

        Ok(())
    }

    #[instrument(skip_all)]
    async fn run_shutdown<S>(&mut self, session: &mut S, on_failure: bool) -> miette::Result<()>
    where
        S: AppSession,
    {
        if on_failure {
            trace!("Running shutdown phase (because another phase failed)");
        } else {
            trace!("Running shutdown phase");
        }

        self.phase = AppPhase::Shutdown;
        self.handle_exit_code(session.shutdown().await?);

        if self.exit_code.is_none() {
            self.exit_code = Some(1);
        }

        Ok(())
    }

    fn handle_exit_code(&mut self, code: Option<u8>) {
        if let Some(code) = code {
            self.exit_code = Some(code);
        }
    }
}
