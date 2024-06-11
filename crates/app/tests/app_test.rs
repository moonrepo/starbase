#![allow(dead_code)]

use async_trait::async_trait;
use miette::{bail, IntoDiagnostic};
use starbase::{App, AppPhase, AppResult, AppSession};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

#[derive(Clone, Debug, Default)]
struct TestSession {
    pub contexts: Arc<RwLock<Vec<String>>>,
    pub order: Arc<RwLock<Vec<String>>>,
    pub error_in_phase: Option<AppPhase>,
}

impl TestSession {
    pub fn get_contexts(self) -> Vec<String> {
        let lock = Arc::into_inner(self.contexts).unwrap();
        lock.into_inner()
    }

    pub fn get_order(self) -> Vec<String> {
        let lock = Arc::into_inner(self.order).unwrap();
        lock.into_inner()
    }
}

#[async_trait]
impl AppSession for TestSession {
    async fn startup(&mut self) -> AppResult {
        self.order.write().await.push("startup".into());

        if self.error_in_phase == Some(AppPhase::Startup) {
            bail!("error in startup");
        }

        Ok(())
    }

    async fn analyze(&mut self) -> AppResult {
        self.order.write().await.push("analyze".into());

        if self.error_in_phase == Some(AppPhase::Analyze) {
            bail!("error in analyze");
        }

        Ok(())
    }

    async fn execute(&mut self) -> AppResult {
        self.order.write().await.push("execute".into());

        if self.error_in_phase == Some(AppPhase::Execute) {
            bail!("error in execute");
        }

        let context = self.contexts.clone();

        context.write().await.push("execute".into());

        task::spawn(async move {
            context.write().await.push("async-task".into());
        })
        .await
        .into_diagnostic()?;

        Ok(())
    }

    async fn shutdown(&mut self) -> AppResult {
        self.order.write().await.push("shutdown".into());

        if self.error_in_phase == Some(AppPhase::Shutdown) {
            bail!("error in shutdown");
        }

        self.contexts.write().await.push("shutdown".into());

        Ok(())
    }
}

async fn noop<S>(_session: S) -> AppResult {
    Ok(())
}

#[tokio::test]
async fn runs_in_order() {
    let mut session = TestSession::default();

    App::default()
        .run_with_session(&mut session, noop)
        .await
        .unwrap();

    assert_eq!(
        session.get_order(),
        vec!["startup", "analyze", "execute", "shutdown"]
    );
}

#[tokio::test]
async fn runs_other_contexts() {
    let mut session = TestSession::default();

    App::default()
        .run_with_session(&mut session, noop)
        .await
        .unwrap();

    assert_eq!(
        session.get_contexts(),
        vec!["execute", "async-task", "shutdown"]
    );
}

mod startup {
    use super::*;

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut session = TestSession {
            error_in_phase: Some(AppPhase::Startup),
            ..Default::default()
        };

        let error = App::default().run_with_session(&mut session, noop).await;

        assert!(error.is_err());
        assert_eq!(error.unwrap_err().to_string(), "error in startup");
        assert_eq!(session.get_order(), vec!["startup", "shutdown"]);
    }
}

mod analyze {
    use super::*;

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut session = TestSession {
            error_in_phase: Some(AppPhase::Analyze),
            ..Default::default()
        };

        let error = App::default().run_with_session(&mut session, noop).await;

        assert!(error.is_err());
        assert_eq!(error.unwrap_err().to_string(), "error in analyze");
        assert_eq!(session.get_order(), vec!["startup", "analyze", "shutdown"]);
    }
}

mod execute {
    use super::*;

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut session = TestSession {
            error_in_phase: Some(AppPhase::Execute),
            ..Default::default()
        };

        let error = App::default().run_with_session(&mut session, noop).await;

        assert!(error.is_err());
        assert_eq!(error.unwrap_err().to_string(), "error in execute");
        assert_eq!(
            session.get_order(),
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }
}

mod shutdown {
    use super::*;

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut session = TestSession {
            error_in_phase: Some(AppPhase::Shutdown),
            ..Default::default()
        };

        let error = App::default().run_with_session(&mut session, noop).await;

        assert!(error.is_err());
        assert_eq!(error.unwrap_err().to_string(), "error in shutdown");
        assert_eq!(
            session.get_order(),
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }
}
