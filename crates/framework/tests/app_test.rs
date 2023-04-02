use starship::{App, Emitters, Resources, States, SystemResult};
use starship_macros::*;
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
    .await?;

    Ok(())
}

mod initializers {
    use super::*;

    #[tokio::test]
    async fn runs_in_order() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.add_initializer(
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
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(system_with_thread);
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await?;

                Ok(())
            },
        );
        app.add_initializer(system);

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
}

mod analyzers {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.add_analyzer(
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
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.add_analyzer(
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
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_analyzer(system_with_thread);
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await?;

                Ok(())
            },
        );
        app.add_analyzer(system);

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
    async fn runs_after_initializers() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("initializer".into());
                Ok(())
            },
        );
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyzer".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(states.get::<RunOrder>().0, vec!["initializer", "analyzer"]);
    }
}

mod executors {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.add_executor(
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
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.add_executor(
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
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_executor(system_with_thread);
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await?;

                Ok(())
            },
        );
        app.add_executor(system);

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
    async fn runs_after_analyzers() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("initializer".into());
                Ok(())
            },
        );
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyzer".into());
                Ok(())
            },
        );
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("executor".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(
            states.get::<RunOrder>().0,
            vec!["initializer", "analyzer", "executor"]
        );
    }
}

mod finalizers {
    use super::*;

    #[tokio::test]
    async fn runs_in_parallel() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_finalizer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                let mut states = states.write().await;
                let order = states.get_mut::<RunOrder>();
                order.push("1".into());
                Ok(())
            },
        );
        app.add_finalizer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("2".into());
                Ok(())
            },
        );
        app.add_finalizer(
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
        app.add_finalizer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states.write().await.get_mut::<RunOrder>().push("4".into());
                Ok(())
            },
        );
        app.add_finalizer(
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
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_finalizer(system_with_thread);
        app.add_finalizer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                task::spawn(async move {
                    sleep(Duration::from_millis(100)).await;

                    let mut states = states.write().await;
                    let order = states.get_mut::<RunOrder>();
                    order.push("async-closure-thread".into());
                })
                .await?;

                Ok(())
            },
        );
        app.add_finalizer(system);

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
    async fn runs_after_analyzers() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("initializer".into());
                Ok(())
            },
        );
        app.add_analyzer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("analyzer".into());
                Ok(())
            },
        );
        app.add_executor(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("executor".into());
                Ok(())
            },
        );
        app.add_finalizer(
            |states: States, _resources: Resources, _emitters: Emitters| async move {
                states
                    .write()
                    .await
                    .get_mut::<RunOrder>()
                    .push("finalizer".into());
                Ok(())
            },
        );

        let states = app.run().await.unwrap();

        assert_eq!(
            states.get::<RunOrder>().0,
            vec!["initializer", "analyzer", "executor", "finalizer"]
        );
    }
}
