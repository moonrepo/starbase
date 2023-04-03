use starship::{App, Emitters, Resources, Result, State, States};

#[derive(Debug, State)]
struct Test(String);

async fn init1(states: States, _resources: Resources, _emitters: Emitters) -> Result<()> {
    let mut states = states.write().await;
    println!("initialize 1");
    states.set(Test("original".into()));
    Ok(())
}

async fn init2(states: States, _resources: Resources, _emitters: Emitters) -> Result<()> {
    tokio::spawn(async move {
        let states = states.read().await;
        println!("initialize 2");
        let state = states.get::<Test>();
        dbg!(state);
    })
    .await?;

    Ok(())
}

async fn anal1(states: States, _resources: Resources, _emitters: Emitters) -> Result<()> {
    let mut states = states.write().await;
    println!("analyze");
    let state = states.get_mut::<Test>();
    **state = "mutated".to_string();
    Ok(())
}

async fn fin(states: States, _resources: Resources, _emitters: Emitters) -> Result<()> {
    let states = states.read().await;
    println!("finalize");
    let state = states.get::<Test>();
    dbg!(state);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.shutdown(fin);
    app.analyze(anal1);
    app.startup(init1);
    app.startup(init2);

    let ctx = app.run().await.unwrap();
    dbg!(ctx);
}
