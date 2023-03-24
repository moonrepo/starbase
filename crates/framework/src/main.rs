use core::future::Future;
use starship::{App, Context, ContextManager};
use std::{thread::sleep, time::Duration};

struct One;
struct Two;
struct Three;

async fn test1(ctx: Context) -> anyhow::Result<()> {
    let mut ctx = ctx.write().await;
    println!("init 1");
    // context.state::<One>()?;
    ctx.set_state(One);
    Ok(())
}

async fn test2(ctx: Context) -> anyhow::Result<()> {
    println!("init 2");
    // context.write().await.state.set(Two);
    Ok(())
}

async fn test3(ctx: Context) -> anyhow::Result<()> {
    println!("analyze 1");
    // context.write().await.state.set(Three);
    Ok(())
}

async fn test_system(ctx: Context) -> anyhow::Result<()> {
    println!("SYSTEM");
    dbg!(ctx.read().await);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_finalizer(test_system);
    app.add_analyzer(test3);
    app.add_initializer(test1);
    app.add_initializer(test2);

    app.run().await.unwrap();
}
