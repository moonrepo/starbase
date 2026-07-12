#![allow(dead_code)]

use async_trait::async_trait;
use starbase::{App, AppExitCode, AppPhase, AppResult, AppSession};
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;

/// A minimal error type that only satisfies `Debug + Display + Send + 'static`,
/// demonstrating that a session error does not need to implement
/// `std::error::Error` (so `miette::Report`, `anyhow::Error`, etc. also work).
#[derive(Debug)]
struct TestError(String);

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, Default)]
struct TestSession {
    pub contexts: Arc<RwLock<Vec<String>>>,
    pub order: Arc<RwLock<Vec<String>>>,
    pub error_in_phase: Option<AppPhase>,
    pub exit_code: Option<AppExitCode>,
    pub exit_in_phase: Option<AppPhase>,
    pub set_code_in_phase: Option<(AppPhase, u8)>,
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

    fn maybe_set_code(&self, phase: AppPhase) {
        if let Some((set_phase, code)) = self.set_code_in_phase
            && set_phase == phase
        {
            self.exit_code
                .as_ref()
                .expect("bootstrap was not called")
                .set(code);
        }
    }
}

#[async_trait]
impl AppSession for TestSession {
    type Error = TestError;

    async fn bootstrap(&mut self, exit_code: AppExitCode) {
        self.exit_code = Some(exit_code);
    }

    async fn startup(&mut self) -> AppResult<Self::Error> {
        dbg!(1);

        self.order.write().await.push("startup".into());
        self.maybe_set_code(AppPhase::Startup);

        if self.error_in_phase == Some(AppPhase::Startup) {
            return Err(TestError("error in startup".into()));
        }

        if self.exit_in_phase == Some(AppPhase::Startup) {
            return Ok(Some(1));
        }

        Ok(None)
    }

    async fn analyze(&mut self) -> AppResult<Self::Error> {
        dbg!(2);

        self.order.write().await.push("analyze".into());
        self.maybe_set_code(AppPhase::Analyze);

        if self.error_in_phase == Some(AppPhase::Analyze) {
            return Err(TestError("error in analyze".into()));
        }

        if self.exit_in_phase == Some(AppPhase::Analyze) {
            return Ok(Some(2));
        }

        Ok(None)
    }

    async fn execute(&mut self) -> AppResult<Self::Error> {
        dbg!(3);

        self.order.write().await.push("execute".into());
        self.maybe_set_code(AppPhase::Execute);

        if self.error_in_phase == Some(AppPhase::Execute) {
            return Err(TestError("error in execute".into()));
        }

        if self.exit_in_phase == Some(AppPhase::Execute) {
            return Ok(Some(3));
        }

        let context = self.contexts.clone();

        context.write().await.push("execute".into());

        task::spawn(async move {
            context.write().await.push("async-task".into());
        })
        .await
        .map_err(|error| TestError(error.to_string()))?;

        Ok(None)
    }

    async fn shutdown(&mut self) -> AppResult<Self::Error> {
        dbg!(4);

        self.order.write().await.push("shutdown".into());
        self.maybe_set_code(AppPhase::Shutdown);

        if self.error_in_phase == Some(AppPhase::Shutdown) {
            return Err(TestError("error in shutdown".into()));
        }

        if self.exit_in_phase == Some(AppPhase::Shutdown) {
            return Ok(Some(4));
        }

        self.contexts.write().await.push("shutdown".into());

        Ok(None)
    }
}

async fn noop(_session: TestSession) -> AppResult<TestError> {
    Ok(None)
}

async fn noop_code(_session: TestSession) -> AppResult<TestError> {
    Ok(Some(5))
}

#[tokio::test]
async fn runs_in_order() {
    let mut session = TestSession::default();

    App::default()
        .run_with_session(&mut session, noop)
        .await
        .into_result()
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
        .into_result()
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

        let result = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "error in startup");
        assert_eq!(session.get_order(), vec!["startup", "shutdown"]);
    }

    #[tokio::test]
    async fn returns_exit_code_1() {
        let mut session = TestSession {
            exit_in_phase: Some(AppPhase::Startup),
            ..Default::default()
        };

        let code = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result()
            .unwrap();

        assert_eq!(code, 1);
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

        let result = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "error in analyze");
        assert_eq!(session.get_order(), vec!["startup", "analyze", "shutdown"]);
    }

    #[tokio::test]
    async fn returns_exit_code_2() {
        let mut session = TestSession {
            exit_in_phase: Some(AppPhase::Analyze),
            ..Default::default()
        };

        let code = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result()
            .unwrap();

        assert_eq!(code, 2);
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

        let result = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "error in execute");
        assert_eq!(
            session.get_order(),
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }

    #[tokio::test]
    async fn returns_exit_code_3() {
        let mut session = TestSession {
            exit_in_phase: Some(AppPhase::Execute),
            ..Default::default()
        };

        let code = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result()
            .unwrap();

        assert_eq!(code, 3);
    }

    #[tokio::test]
    async fn returns_exit_code_5() {
        let mut session = TestSession::default();

        let code = App::default()
            .run_with_session(&mut session, noop_code)
            .await
            .into_result()
            .unwrap();

        assert_eq!(code, 5);
    }
}

mod shared_exit_code {
    use super::*;

    #[tokio::test]
    async fn defaults_to_zero_when_unset() {
        let mut session = TestSession::default();

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 0);
    }

    #[tokio::test]
    async fn preserves_max_code_255() {
        let mut session = TestSession {
            set_code_in_phase: Some((AppPhase::Analyze, 255)),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 255);
    }

    #[tokio::test]
    async fn shares_code_set_inside_the_execute_op() {
        let mut session = TestSession::default();

        let outcome = App::default()
            .run_with_session(&mut session, |session: TestSession| async move {
                session.exit_code.as_ref().unwrap().set(42);

                Ok(None)
            })
            .await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 42);
    }

    #[tokio::test]
    async fn shares_code_set_inside_the_background_execute() {
        let mut session = TestSession {
            set_code_in_phase: Some((AppPhase::Execute, 7)),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 7);
    }

    #[tokio::test]
    async fn keeps_explicit_code_when_a_phase_errors() {
        let mut session = TestSession {
            set_code_in_phase: Some((AppPhase::Startup, 42)),
            error_in_phase: Some(AppPhase::Execute),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_some());
        assert_eq!(outcome.exit_code, 42);
    }

    #[tokio::test]
    async fn forces_one_when_code_is_zero_and_a_phase_errors() {
        let mut session = TestSession {
            set_code_in_phase: Some((AppPhase::Startup, 0)),
            error_in_phase: Some(AppPhase::Execute),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_some());
        assert_eq!(outcome.exit_code, 1);
    }

    #[tokio::test]
    async fn forces_one_when_code_is_zero_and_shutdown_errors() {
        let mut session = TestSession {
            set_code_in_phase: Some((AppPhase::Startup, 0)),
            error_in_phase: Some(AppPhase::Shutdown),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_some());
        assert_eq!(outcome.exit_code, 1);
    }

    #[tokio::test]
    async fn overwrites_phase_returned_code_with_later_set() {
        let mut session = TestSession {
            // Startup returns `Ok(Some(1))`, then shutdown sets 9 directly
            exit_in_phase: Some(AppPhase::Startup),
            set_code_in_phase: Some((AppPhase::Shutdown, 9)),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 9);
    }

    #[tokio::test]
    async fn overwrites_earlier_set_with_phase_returned_code() {
        let mut session = TestSession {
            // Startup sets 9 directly, then shutdown returns `Ok(Some(4))`
            set_code_in_phase: Some((AppPhase::Startup, 9)),
            exit_in_phase: Some(AppPhase::Shutdown),
            ..Default::default()
        };

        let outcome = App::default().run_with_session(&mut session, noop).await;

        assert!(outcome.error.is_none());
        assert_eq!(outcome.exit_code, 4);
    }
}

mod miette_result {
    use super::*;
    use starbase::AppRunOutcome;
    use std::process::ExitCode;

    fn create_outcome(error: Option<TestError>, exit_code: u8) -> AppRunOutcome<TestError> {
        AppRunOutcome {
            last_phase: AppPhase::Shutdown,
            error,
            exit_code,
        }
    }

    #[test]
    fn preserves_codes_above_one_by_rendering_manually() {
        let result = create_outcome(Some(TestError("fail".into())), 8).into_miette_result();

        assert_eq!(
            format!("{:?}", result.unwrap()),
            format!("{:?}", ExitCode::from(8))
        );
    }

    #[test]
    fn returns_error_for_code_one() {
        let result = create_outcome(Some(TestError("fail".into())), 1).into_miette_result();

        assert!(result.is_err());
    }

    #[test]
    fn passes_through_success_codes() {
        let result = create_outcome(None, 0).into_miette_result();

        assert_eq!(
            format!("{:?}", result.unwrap()),
            format!("{:?}", ExitCode::from(0))
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

        let result = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "error in shutdown");
        assert_eq!(
            session.get_order(),
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }

    #[tokio::test]
    async fn returns_exit_code_4() {
        let mut session = TestSession {
            exit_in_phase: Some(AppPhase::Shutdown),
            ..Default::default()
        };

        let code = App::default()
            .run_with_session(&mut session, noop)
            .await
            .into_result()
            .unwrap();

        assert_eq!(code, 4);
    }
}
