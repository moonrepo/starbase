use starship::{App, Context, Result, State};
use std::time::Duration;
use tokio::task;
use tokio::time::sleep;

#[derive(State)]
struct RunOrder(pub Vec<String>);

async fn setup_state(ctx: Context) -> Result<()> {
    ctx.write().await.add_state(RunOrder(vec![]));

    Ok(())
}

async fn system(ctx: Context) -> Result<()> {
    ctx.write()
        .await
        .state_mut::<RunOrder>()
        .push("async-function".into());

    Ok(())
}

async fn system_with_thread(ctx: Context) -> Result<()> {
    task::spawn(async move {
        ctx.write()
            .await
            .state_mut::<RunOrder>()
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
        app.add_initializer(|ctx: Context| async move {
            let mut ctx = ctx.write().await;
            let order = ctx.state_mut::<RunOrder>();
            order.push("1".into());
            Ok(())
        });
        app.add_initializer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().push("2".into());
            Ok(())
        });
        app.add_initializer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().0.push("3".into());
            Ok(())
        });

        let ctx = app.run().await.unwrap();

        assert_eq!(ctx.state::<RunOrder>().0, vec!["1", "2", "3"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_initializer(system_with_thread);
        app.add_initializer(|ctx: Context| async move {
            task::spawn(async move {
                let mut ctx = ctx.write().await;
                let order = ctx.state_mut::<RunOrder>();
                order.push("async-closure-thread".into());
            })
            .await?;

            Ok(())
        });
        app.add_initializer(system);

        let ctx = app.run().await.unwrap();

        assert_eq!(
            ctx.state::<RunOrder>().0,
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
        app.add_analyzer(|ctx: Context| async move {
            let mut ctx = ctx.write().await;
            let order = ctx.state_mut::<RunOrder>();
            order.push("1".into());
            Ok(())
        });
        app.add_analyzer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().push("2".into());
            Ok(())
        });
        app.add_analyzer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().0.push("3".into());
            Ok(())
        });
        app.add_analyzer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().push("4".into());
            Ok(())
        });
        app.add_analyzer(|ctx: Context| async move {
            ctx.write().await.state_mut::<RunOrder>().0.push("5".into());
            Ok(())
        });

        let ctx = app.run().await.unwrap();

        assert_ne!(ctx.state::<RunOrder>().0, vec!["1", "2", "3", "5", "5"]);
    }

    #[tokio::test]
    async fn supports_threads() {
        let mut app = App::default();
        app.add_initializer(setup_state);
        app.add_analyzer(system_with_thread);
        app.add_analyzer(|ctx: Context| async move {
            task::spawn(async move {
                sleep(Duration::from_millis(100)).await;

                let mut ctx = ctx.write().await;
                let order = ctx.state_mut::<RunOrder>();
                order.push("async-closure-thread".into());
            })
            .await?;

            Ok(())
        });
        app.add_analyzer(system);

        let ctx = app.run().await.unwrap();

        assert_ne!(
            ctx.state::<RunOrder>().0,
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
        app.add_initializer(|ctx: Context| async move {
            ctx.write()
                .await
                .state_mut::<RunOrder>()
                .push("initializer".into());
            Ok(())
        });
        app.add_analyzer(|ctx: Context| async move {
            ctx.write()
                .await
                .state_mut::<RunOrder>()
                .push("analyzer".into());
            Ok(())
        });

        let ctx = app.run().await.unwrap();

        assert_eq!(ctx.state::<RunOrder>().0, vec!["initializer", "analyzer"]);
    }
}
