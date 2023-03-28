use starship::{App, Context};
use starship_macros::*;

#[derive(Debug, State)]
struct Test(String);

async fn init1(ctx: Context) -> anyhow::Result<()> {
    let mut ctx = ctx.write().await;
    println!("initialize 1");
    ctx.add_state(Test("original".into()));
    Ok(())
}

async fn init2(ctx: Context) -> anyhow::Result<()> {
    let ctx = ctx.read().await;
    println!("initialize 2");
    let state = ctx.state::<Test>()?;
    dbg!(state);

    Ok(())
}

async fn anal1(ctx: Context) -> anyhow::Result<()> {
    let mut ctx = ctx.write().await;
    println!("analyze");
    let state = ctx.state_mut::<Test>()?;
    **state = "mutated".to_string();
    Ok(())
}

async fn fin(ctx: Context) -> anyhow::Result<()> {
    let ctx = ctx.read().await;
    println!("finalize");
    let state = ctx.state::<Test>()?;
    dbg!(state);
    dbg!(ctx);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::default();
    app.add_finalizer(fin);
    app.add_analyzer(anal1);
    app.add_initializer(init1);
    app.add_initializer(init2);
    app.run().await.unwrap();
}
