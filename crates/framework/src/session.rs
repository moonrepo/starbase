use std::fmt::Debug;

pub type AppResult<T = ()> = miette::Result<T>;

#[async_trait::async_trait]
pub trait AppSession: Clone + Debug + Send + Sync {
    /// Run operations at the start of the application process to setup
    /// the initial state of the session.
    async fn startup(&mut self) -> AppResult {
        Ok(())
    }

    /// Run operations after the session state has been created,
    /// but before the main execution.
    async fn analyze(&mut self) -> AppResult {
        Ok(())
    }

    /// Run operations in the background of the main execution. The main
    /// execution is defined in [`App.run`], _not_ here.
    async fn execute(&mut self) -> AppResult {
        Ok(())
    }

    /// Run operations on success or failure of the other phases.
    async fn shutdown(&mut self) -> AppResult {
        Ok(())
    }
}
