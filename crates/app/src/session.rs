/// Generic result for session operations.
pub type AppResult = miette::Result<Option<u8>>;

/// A session that is passed to each application run.
#[async_trait::async_trait]
pub trait AppSession: Clone + Send + Sync {
    /// Run operations at the start of the application process to setup
    /// the initial state of the session.
    async fn startup(&mut self) -> AppResult {
        Ok(None)
    }

    /// Run operations after the session state has been created,
    /// but before the main execution.
    async fn analyze(&mut self) -> AppResult {
        Ok(None)
    }

    /// Run operations in the background of the main execution. The main
    /// execution is defined in [`App#run`](crate::App), _not_ here.
    async fn execute(&mut self) -> AppResult {
        Ok(None)
    }

    /// Run operations on success or failure of the other phases.
    async fn shutdown(&mut self) -> AppResult {
        Ok(None)
    }
}
