use std::fmt::Debug;

pub type AppResult<T = ()> = miette::Result<T>;

#[async_trait::async_trait]
pub trait AppSession: Debug + Send + Sync {
    async fn startup(&mut self) -> AppResult {
        Ok(())
    }

    async fn analyze(&mut self) -> AppResult {
        Ok(())
    }

    async fn execute(&self) -> AppResult {
        Ok(())
    }

    async fn shutdown(&mut self) -> AppResult {
        Ok(())
    }
}
