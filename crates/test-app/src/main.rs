use starship::{App, Context, Result, State};

#[derive(Debug, State)]
struct Test(String);

async fn init1(ctx: Context) -> Result<()> {
    let mut ctx = ctx.write().await;
    println!("initialize 1");
    ctx.add_state(Test("original".into()));
    Ok(())
}

async fn init2(ctx: Context) -> Result<()> {
    tokio::spawn(async move {
        let ctx = ctx.read().await;
        println!("initialize 2");
        let state = ctx.state::<Test>();
        dbg!(state);
    })
    .await?;

    Ok(())
}

async fn anal1(ctx: Context) -> Result<()> {
    let mut ctx = ctx.write().await;
    println!("analyze");
    let state = ctx.state_mut::<Test>();
    **state = "mutated".to_string();
    Ok(())
}

async fn fin(ctx: Context) -> Result<()> {
    let ctx = ctx.read().await;
    println!("finalize");
    let state = ctx.state::<Test>();
    dbg!(state);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::default();
    app.add_finalizer(fin);
    app.add_analyzer(anal1);
    app.add_initializer(init1);
    app.add_initializer(init2);

    let ctx = app.run().await.unwrap();
    dbg!(ctx);
}
