use starship::{App, Emitters, IntoDiagnostic, MainResult, Resources, State, States, SystemResult};

#[derive(Debug, State)]
struct Test(String);

async fn start1(states: States, _resources: Resources, _emitters: Emitters) -> SystemResult {
    let mut states = states.write().await;
    println!("startup 1");
    states.set(Test("original".into()));
    Ok(())
}

async fn start2(states: States, _resources: Resources, _emitters: Emitters) -> SystemResult {
    tokio::spawn(async move {
        let states = states.read().await;
        println!("startup 2");
        let state = states.get::<Test>();
        dbg!(state);
    })
    .await
    .into_diagnostic()?;

    Ok(())
}

async fn anal1(states: States, _resources: Resources, _emitters: Emitters) -> SystemResult {
    let mut states = states.write().await;
    println!("analyze");
    let state = states.get_mut::<Test>();
    **state = "mutated".to_string();
    Ok(())
}

async fn fin(states: States, _resources: Resources, _emitters: Emitters) -> SystemResult {
    let states = states.read().await;
    println!("shutdown");
    let state = states.get::<Test>();
    dbg!(state);

    Ok(())
}

#[tokio::main]
async fn main() -> MainResult {
    let mut app = App::new();
    app.shutdown(fin);
    app.analyze(anal1);
    app.startup(start1);
    app.startup(start2);

    let ctx = app.run().await.unwrap();
    dbg!(ctx);

    Ok(())
}
