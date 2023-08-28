#![allow(dead_code)]

use miette::{bail, IntoDiagnostic};
use starbase::{App, AppPhase, Emitters, Resources, States, SystemResult};
use starbase_macros::*;
use std::time::Duration;
use tokio::task;
use tokio::time::sleep;

#[derive(State)]
struct RunOrder(Vec<String>);

#[system]
async fn setup_state(states: StatesMut) {
    states.set(RunOrder(vec![]));
}

#[system]
async fn system(order: StateMut<RunOrder>) {
    order.push("async-function".into());
}

#[system]
async fn fail() {
    if true {
        bail!("Fail!");
    }
}

async fn system_with_thread(
    states: States,
    _resources: Resources,
    _emitters: Emitters,
) -> SystemResult {
    task::spawn(async move {
        states
            .write()
            .await
            .get_mut::<RunOrder>()
            .push("async-function-thread".into());
    })
    .await
    .into_diagnostic()?;

    Ok(())
}

mod startup {
    use super::*;

    #[tokio::test]
    async fn runs_in_order() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("3".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(states.get::<RunOrder>().0, vec!["1", "2", "3"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(system_with_thread);
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await
                .into_diagnostic()?;

                Ok(())
            },
        );
        app.startup(system);

        let states = app.run().await.unwrap();

        assert_eq!(
            states.get::<RunOrder>().0,
            vec![
                "async-function-thread",
                "async-closure-thread",
                "async-function"
            ]
        );
    }

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(fail);

        let error = app.run().await;

        assert!(error.is_err());
    }
}

mod analyze {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::new();
        app.startup(setup_state);
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("3".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("5".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::new();
        app.startup(setup_state);
        app.analyze(system_with_thread);
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await
                .into_diagnostic()?;

                Ok(())
            },
        );
        app.analyze(system);

        let states = app.run().await.unwrap();

        assert_ne!(
            states.get::<RunOrder>().0,
            vec![
                "async-function-thread",
                "async-closure-thread",
                "async-function"
            ]
        );
    }

    #[tokio::test]
    async fn runs_after_startup() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("startup".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyze".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(states.get::<RunOrder>().0, vec!["startup", "analyze"]);
    }

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut app = App::new();
        app.startup(setup_state);
        app.analyze(fail);

        let error = app.run().await;

        assert!(error.is_err());
    }
}

mod execute {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::new();
        app.startup(setup_state);
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("3".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("5".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::new();
        app.startup(setup_state);
        app.execute(system_with_thread);
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await
                .into_diagnostic()?;

                Ok(())
            },
        );
        app.execute(system);

        let states = app.run().await.unwrap();

        assert_ne!(
            states.get::<RunOrder>().0,
            vec![
                "async-function-thread",
                "async-closure-thread",
                "async-function"
            ]
        );
    }

    #[tokio::test]
    async fn runs_after_analyze() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("startup".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyze".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("execute".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(
            states.get::<RunOrder>().0,
            vec!["startup", "analyze", "execute"]
        );
    }

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut app = App::new();
        app.startup(setup_state);
        app.execute(fail);

        let error = app.run().await;

        assert!(error.is_err());
    }
}

mod execute_with_args {
    use super::*;
    use starbase::{ExecuteArgs, StateInstance};

    #[derive(Debug, Clone)]
    struct TestArgs {
        pub value: u32,
    }

    #[tokio::test]
    async fn can_access_args() {
        let mut app = App::new();
        app.startup(setup_state);
        app.execute_with_args(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let args = {
                    let states = states.read().await;
                    states
                        .get::<ExecuteArgs>()
                        .extract::<TestArgs>()
                        .unwrap()
                        .to_owned()
                };

                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push(format!("{:?}", args));

                Ok(())
            },
            TestArgs { value: 1 },
        );

        let states = app.run().await.unwrap();

        assert_eq!(states.get::<RunOrder>().0, vec!["TestArgs { value: 1 }"]);
    }
}

mod shutdown {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::new();
        app.startup(setup_state);
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("3".into());
                Ok(())
            },
        );
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .0
                    .push("5".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::new();
        app.startup(setup_state);
        app.shutdown(system_with_thread);
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await
                .into_diagnostic()?;

                Ok(())
            },
        );
        app.shutdown(system);

        let states = app.run().await.unwrap();

        assert_ne!(
            states.get::<RunOrder>().0,
            vec![
                "async-function-thread",
                "async-closure-thread",
                "async-function"
            ]
        );
    }

    #[tokio::test]
    async fn runs_after_execute() {
        let mut app = App::new();
        app.startup(setup_state);
        app.startup(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("startup".into());
                Ok(())
            },
        );
        app.analyze(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyze".into());
                Ok(())
            },
        );
        app.execute(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("execute".into());
                Ok(())
            },
        );
        app.shutdown(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("shutdown".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(
            states.get::<RunOrder>().0,
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }

    #[tokio::test]
    async fn bubbles_up_error() {
        let mut app = App::new();
        app.startup(setup_state);
        app.shutdown(fail);

        let error = app.run().await;

        assert!(error.is_err());
    }
}

#[system]
fn extract_app_state(states: StatesMut) {
    let phase = { format!("{:?}", states.get::<AppPhase>().phase) };

    let order = states.get_mut::<RunOrder>();
    order.push(phase);
}

#[tokio::test]
async fn tracks_app_state() {
    let mut app = App::new();
    app.startup(setup_state);

    // This also tests the same system being used multiple times
    app.startup(extract_app_state);
    app.analyze(extract_app_state);
    app.execute(extract_app_state);
    app.shutdown(extract_app_state);

    let states = app.run().await.unwrap();

    assert_eq!(
        states.get::<RunOrder>().0,
        vec!["Startup", "Analyze", "Execute", "Shutdown"]
    );
}
