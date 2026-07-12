use crate::exit_code::AppExitCode;
use crate::session::{AppResult, AppSession};
#[cfg(feature = "tracing")]
use crate::tracing::TracingOptions;
use std::future::Future;
use std::process::ExitCode;
use tokio::spawn;
use tokio::task::JoinHandle;
#[cfg(feature = "tracing")]
use tracing::{instrument, trace};

#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)*) => {};
}

/// A result for `main` that handles errors and exit codes.
#[cfg(feature = "miette")]
pub type MainResult = miette::Result<ExitCode>;

/// Phases of an application's lifecycle.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum AppPhase {
    #[default]
    Startup,
    Analyze,
    Execute,
    Shutdown,
}

/// The outcome of an application run, including the last phase, any error, and the exit code.
/// This type exists to provide a mechanism for failures to define their own exit codes,
/// otherwise [`Err`] handling will swallow it.
#[derive(Debug)]
pub struct AppRunOutcome<E> {
    pub last_phase: AppPhase,
    pub error: Option<E>,
    pub exit_code: u8,
}

impl<E> AppRunOutcome<E> {
    /// Convert the outcome into a standard [`Result`] with a u8 exit code on
    /// success or an error on failure.
    pub fn into_result(self) -> Result<u8, E> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.exit_code),
        }
    }

    /// Convert the outcome into a standard [`Result`] with an [`ExitCode`] on
    /// success or an error on failure.
    pub fn into_exit_result(self) -> Result<ExitCode, E> {
        self.into_result().map(ExitCode::from)
    }

    /// Convert the outcome into a standard [`Result`] with an [`ExitCode`] on
    /// both success and failure, but with pretty error output if the `miette`
    /// feature is enabled. This will preserve the exit code, unlike `miette`'s
    /// default `main` behavior of always returning 1 on error.
    #[cfg(feature = "miette")]
    pub fn into_miette_result(self) -> Result<ExitCode, E>
    where
        E: std::fmt::Debug,
    {
        // If we received a custom exit code that is neither 0 or 1,
        // then we need to manually render the miette pretty output and
        // exit with that code. This is because miette always renders errors
        // with exit code 1, but we want to preserve the actual exit code.
        if self.exit_code > 1 {
            if let Some(error) = self.error {
                eprintln!("{error:?}");
            }

            return Ok(ExitCode::from(self.exit_code));
        }

        self.into_exit_result()
    }
}

/// An application that runs through lifecycles using a session instance.
#[derive(Debug, Default)]
pub struct App {
    phase: AppPhase,
    exit_code: AppExitCode,
}

impl App {
    /// Setup [`miette`] diagnostics by registering error and panic hooks.
    #[cfg(feature = "miette")]
    pub fn setup_diagnostics(&self) {
        crate::diagnostics::setup_miette();
    }

    /// Setup [`tracing`] messages with default options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing_with_defaults(
        &self,
    ) -> crate::tracing::TracingResult<crate::tracing::TracingGuard> {
        self.setup_tracing(TracingOptions::default())
    }

    /// Setup [`tracing`] messages with custom options.
    #[cfg(feature = "tracing")]
    pub fn setup_tracing(
        &self,
        options: TracingOptions,
    ) -> crate::tracing::TracingResult<crate::tracing::TracingGuard> {
        crate::tracing::setup_tracing(options)
    }

    /// Start the application with the provided session and execute all phases
    /// in order. If a phase fails, always run the shutdown phase.
    pub async fn run<S, F, Fut>(self, mut session: S, op: F) -> AppRunOutcome<S::Error>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<S::Error>> + Send + 'static,
    {
        self.run_with_session(&mut session, op).await
    }

    /// Start the application with the provided session and execute all phases
    /// in order. If a phase fails, always run the shutdown phase.
    ///
    /// This method is similar to [`App::run`](#method.run) but doesn't consume
    /// the session, and instead accepts a mutable reference.
    #[cfg_attr(feature = "tracing", instrument(skip_all))]
    pub async fn run_with_session<S, F, Fut>(
        mut self,
        session: &mut S,
        op: F,
    ) -> AppRunOutcome<S::Error>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<S::Error>> + Send + 'static,
    {
        session.bootstrap(self.exit_code.clone()).await;

        // Startup
        if let Err(error) = self.run_startup(session).await {
            return self.run_shutdown(session, Some(error)).await;
        }

        // Analyze
        if let Err(error) = self.run_analyze(session).await {
            return self.run_shutdown(session, Some(error)).await;
        }

        // Execute
        if let Err(error) = self.run_execute(session, op).await {
            return self.run_shutdown(session, Some(error)).await;
        }

        // Shutdown
        self.run_shutdown(session, None).await
    }

    // Private

    #[cfg_attr(feature = "tracing", instrument(skip_all))]
    async fn run_startup<S>(&mut self, session: &mut S) -> Result<(), S::Error>
    where
        S: AppSession,
    {
        trace!("Running startup phase");

        self.phase = AppPhase::Startup;
        self.handle_exit_code(session.startup().await?);

        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip_all))]
    async fn run_analyze<S>(&mut self, session: &mut S) -> Result<(), S::Error>
    where
        S: AppSession,
    {
        trace!("Running analyze phase");

        self.phase = AppPhase::Analyze;
        self.handle_exit_code(session.analyze().await?);

        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip_all))]
    async fn run_execute<S, F, Fut>(&mut self, session: &mut S, op: F) -> Result<(), S::Error>
    where
        S: AppSession + 'static,
        F: FnOnce(S) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<S::Error>> + Send + 'static,
    {
        trace!("Running execute phase");

        self.phase = AppPhase::Execute;

        let fg_session = session.clone();
        let mut bg_session = session.clone();

        // Run the main execution on a spawned task instead of inline on the
        // caller's future. The runtime drives the top-level future on the main
        // thread (via `block_on`), and on some platforms that thread has a much
        // smaller stack than spawned worker threads (~1MB on Windows vs the
        // ~8MB main stack on Linux/macOS, and tokio's ~2MB workers). Deep
        // workloads such as WASM plugin execution can overflow the small main
        // stack, so move both the foreground and background work to workers.
        let fg_handle: JoinHandle<AppResult<S::Error>> =
            spawn(Box::pin(async move { op(fg_session).await }));
        let bg_handle: JoinHandle<AppResult<S::Error>> =
            spawn(Box::pin(async move { bg_session.execute().await }));

        match fg_handle.await {
            Ok(Ok(code)) => self.handle_exit_code(code),
            Ok(Err(error)) => {
                bg_handle.abort();
                return Err(error);
            }
            Err(error) => {
                bg_handle.abort();
                std::panic::resume_unwind(error.into_panic());
            }
        };

        match bg_handle.await {
            Ok(Ok(code)) => self.handle_exit_code(code),
            Ok(Err(error)) => return Err(error),
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        };

        Ok(())
    }

    #[cfg_attr(feature = "tracing", instrument(skip_all))]
    async fn run_shutdown<S>(
        &mut self,
        session: &mut S,
        error: Option<S::Error>,
    ) -> AppRunOutcome<S::Error>
    where
        S: AppSession,
    {
        #[allow(unused)]
        if let Some(error) = &error {
            trace!("Running shutdown phase (because another phase failed): {error}");
        } else {
            trace!("Running shutdown phase");
        }

        let last_phase = self.phase;

        self.phase = AppPhase::Shutdown;

        match session.shutdown().await {
            Ok(code) => {
                self.handle_exit_code(code);
            }
            Err(error) => {
                trace!("Shutdown phase failed with error: {error}");

                return AppRunOutcome {
                    last_phase: self.phase,
                    exit_code: self.get_exit_code(Some(&error)),
                    error: Some(error),
                };
            }
        };

        AppRunOutcome {
            last_phase,
            exit_code: self.get_exit_code(error.as_ref()),
            error,
        }
    }

    fn handle_exit_code(&mut self, code: Option<u8>) {
        if let Some(code) = code {
            trace!(code, "Setting exit code");

            self.exit_code.set(code);
        }
    }

    fn get_exit_code<E>(&self, error: Option<E>) -> u8 {
        let mut exit_code = self.exit_code.get();

        if error.is_some() && exit_code.is_none_or(|code| code == 0) {
            exit_code = Some(1);
        }

        exit_code.unwrap_or_default()
    }
}
