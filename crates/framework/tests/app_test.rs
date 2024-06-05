#![allow(dead_code)]

use async_trait::async_trait;
use miette::{bail, IntoDiagnostic};
use starbase::{App, AppPhase, AppResult, AppSession};
use std::time::Duration;
use tokio::task;
use tokio::time::sleep;

#[derive(Clone, Debug, Default)]
struct TestSession {
    pub order: Vec<String>,
    pub error_in_phase: Option<AppPhase>,
}

#[async_trait]
impl AppSession for TestSession {
    async fn startup(&mut self) -> AppResult {
        self.order.push("startup".into());

        if self.error_in_phase == Some(AppPhase::Startup) {
            bail!("error in startup");
        }

        Ok(())
    }

    async fn analyze(&mut self) -> AppResult {
        self.order.push("analyze".into());

        if self.error_in_phase == Some(AppPhase::Analyze) {
            bail!("error in analyze");
        }

        Ok(())
    }

    async fn execute(&mut self) -> AppResult {
        self.order.push("execute".into());

        if self.error_in_phase == Some(AppPhase::Execute) {
            bail!("error in execute");
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> AppResult {
        self.order.push("shutdown".into());

        if self.error_in_phase == Some(AppPhase::Shutdown) {
            bail!("error in shutdown");
        }

        Ok(())
    }
}

// async fn system_with_thread(
//     states: States,
//     _resources: Resources,
//     _emitters: Emitters,
// ) -> SystemResult {
//     task::spawn(async move {
//         states
//             .get::<RunOrder>()
//             .write()
//             .push("async-function-thread".into());
//     })
//     .await
//     .into_diagnostic()?;

//     Ok(())
// }

async fn noop<S>(_session: S) -> AppResult {
    Ok(())
}

mod startup {
    use super::*;

    #[tokio::test]
    async fn runs_in_order() {
        let session = App::default()
            .run(TestSession::default(), noop)
            .await
            .unwrap();

        assert_eq!(
            session.order,
            vec!["startup", "analyze", "execute", "shutdown"]
        );
    }

    // #[tokio::test]
    // async fn supports_threads() {
    //     let mut app = App::new();
    //     app.startup(setup_state);
    //     app.startup(system_with_thread);
    //     app.startup(
    //         |states: States, _resources: Resources, _emitters: Emitters| async move {
    //             task::spawn(async move {
    //                 let mut order = states.get::<RunOrder>();
    //                 order.write().push("async-closure-thread".into());
    //             })
    //             .await
    //             .into_diagnostic()?;

    //             Ok(())
    //         },
    //     );
    //     app.startup(system);

    //     let states = app.run().await.unwrap();

    //     assert_eq!(
    //         states.get::<RunOrder>().0,
    //         vec![
    //             "async-function-thread",
    //             "async-closure-thread",
    //             "async-function"
    //         ]
    //     );
    // }

    #[tokio::test]
    async fn bubbles_up_error() {
        let error = App::default()
            .run(
                TestSession {
                    error_in_phase: Some(AppPhase::Startup),
                    ..Default::default()
                },
                noop,
            )
            .await;

        assert!(error.is_err());
        assert_eq!(error.unwrap_err().to_string(), "error in startup");
    }
}

// mod analyze {
//     use super::*;

//     #[tokio::test]
//     async fn runs_in_parallel() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 let mut order = states.get::<RunOrder>();
//                 order.push("1".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("2".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("3".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("4".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("5".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
//     }

//     #[tokio::test]
//     async fn supports_threads() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.analyze(system_with_thread);
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 task::spawn(async move {
//                     sleep(Duration::from_millis(100)).await;

//                     let mut order = states.get::<RunOrder>();
//                     order.push("async-closure-thread".into());
//                 })
//                 .await
//                 .into_diagnostic()?;

//                 Ok(())
//             },
//         );
//         app.analyze(system);

//         let states = app.run().await.unwrap();

//         assert_ne!(
//             states.get::<RunOrder>().0,
//             vec![
//                 "async-function-thread",
//                 "async-closure-thread",
//                 "async-function"
//             ]
//         );
//     }

//     #[tokio::test]
//     async fn runs_after_startup() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.startup(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("startup".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("analyze".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_eq!(states.get::<RunOrder>().0, vec!["startup", "analyze"]);
//     }

//     #[tokio::test]
//     async fn bubbles_up_error() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.analyze(fail);

//         let error = app.run().await;

//         assert!(error.is_err());
//     }
// }

// mod execute {
//     use super::*;

//     #[tokio::test]
//     async fn runs_in_parallel() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 let mut order = states.get::<RunOrder>();
//                 order.push("1".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("2".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("3".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("4".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("5".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
//     }

//     #[tokio::test]
//     async fn supports_threads() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.execute(system_with_thread);
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 task::spawn(async move {
//                     sleep(Duration::from_millis(100)).await;

//                     let mut order = states.get::<RunOrder>();
//                     order.push("async-closure-thread".into());
//                 })
//                 .await
//                 .into_diagnostic()?;

//                 Ok(())
//             },
//         );
//         app.execute(system);

//         let states = app.run().await.unwrap();

//         assert_ne!(
//             states.get::<RunOrder>().0,
//             vec![
//                 "async-function-thread",
//                 "async-closure-thread",
//                 "async-function"
//             ]
//         );
//     }

//     #[tokio::test]
//     async fn runs_after_analyze() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.startup(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("startup".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("analyze".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("execute".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_eq!(
//             states.get::<RunOrder>().0,
//             vec!["startup", "analyze", "execute"]
//         );
//     }

//     #[tokio::test]
//     async fn bubbles_up_error() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.execute(fail);

//         let error = app.run().await;

//         assert!(error.is_err());
//     }
// }

// mod execute_with_args {
//     use super::*;
//     use starbase::{ExecuteArgs, StateInstance};

//     #[derive(Debug, Clone)]
//     struct TestArgs {
//         pub value: u32,
//     }

//     #[tokio::test]
//     async fn can_access_args() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.execute_with_args(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 let args = { states.get::<ExecuteArgs>().extract::<TestArgs>().unwrap() };

//                 states.get::<RunOrder>().write().push(format!("{:?}", args));

//                 Ok(())
//             },
//             TestArgs { value: 1 },
//         );

//         let states = app.run().await.unwrap();

//         assert_eq!(states.get::<RunOrder>().0, vec!["TestArgs { value: 1 }"]);
//     }
// }

// mod shutdown {
//     use super::*;

//     #[tokio::test]
//     async fn runs_in_parallel() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 let mut order = states.get::<RunOrder>();
//                 order.push("1".into());
//                 Ok(())
//             },
//         );
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("2".into());
//                 Ok(())
//             },
//         );
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("3".into());
//                 Ok(())
//             },
//         );
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("4".into());
//                 Ok(())
//             },
//         );
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().0.push("5".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_ne!(states.get::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
//     }

//     #[tokio::test]
//     async fn supports_threads() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.shutdown(system_with_thread);
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 task::spawn(async move {
//                     sleep(Duration::from_millis(100)).await;

//                     let mut order = states.get::<RunOrder>();
//                     order.push("async-closure-thread".into());
//                 })
//                 .await
//                 .into_diagnostic()?;

//                 Ok(())
//             },
//         );
//         app.shutdown(system);

//         let states = app.run().await.unwrap();

//         assert_ne!(
//             states.get::<RunOrder>().0,
//             vec![
//                 "async-function-thread",
//                 "async-closure-thread",
//                 "async-function"
//             ]
//         );
//     }

//     #[tokio::test]
//     async fn runs_after_execute() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.startup(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("startup".into());
//                 Ok(())
//             },
//         );
//         app.analyze(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("analyze".into());
//                 Ok(())
//             },
//         );
//         app.execute(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("execute".into());
//                 Ok(())
//             },
//         );
//         app.shutdown(
//             |states: States, _resources: Resources, _emitters: Emitters| async move {
//                 states.get::<RunOrder>().write().push("shutdown".into());
//                 Ok(())
//             },
//         );

//         let states = app.run().await.unwrap();

//         assert_eq!(
//             states.get::<RunOrder>().0,
//             vec!["startup", "analyze", "execute", "shutdown"]
//         );
//     }

//     #[tokio::test]
//     async fn bubbles_up_error() {
//         let mut app = App::new();
//         app.startup(setup_state);
//         app.shutdown(fail);

//         let error = app.run().await;

//         assert!(error.is_err());
//     }
// }

// #[system]
// fn extract_app_state(states: States) {
//     let phase = { format!("{:?}", states.get::<AppPhase>().read().phase) };

//     let mut order = states.get::<RunOrder>();
//     order.write().push(phase);
// }

// #[tokio::test]
// async fn tracks_app_state() {
//     let mut app = App::new();
//     app.startup(setup_state);

//     // This also tests the same system being used multiple times
//     app.startup(extract_app_state);
//     app.analyze(extract_app_state);
//     app.execute(extract_app_state);
//     app.shutdown(extract_app_state);

//     let states = app.run().await.unwrap();

//     assert_eq!(
//         states.get::<RunOrder>().read().0,
//         vec!["Startup", "Analyze", "Execute", "Shutdown"]
//     );
// }

// struct TestExtension;

// impl AppExtension for TestExtension {
//     fn extend(self, app: &mut App) -> miette::Result<()> {
//         app.startup(extract_app_state);
//         app.analyze(extract_app_state);
//         app.execute(extract_app_state);
//         app.shutdown(extract_app_state);

//         Ok(())
//     }
// }

// #[tokio::test]
// async fn extension_can_register_systems() {
//     let mut app = App::new();
//     app.startup(setup_state);
//     app.extend(TestExtension).unwrap();

//     let states = app.run().await.unwrap();

//     assert_eq!(
//         states.get::<RunOrder>().read().0,
//         vec!["Startup", "Analyze", "Execute", "Shutdown"]
//     );
// }
