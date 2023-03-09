use core::future::Future;
use starship::{App, Context};

struct One;
struct Two;
struct Three;

async fn test1(ctx: &mut Context) -> anyhow::Result<()> {
    println!("1");
    ctx.state.set(One);
    Ok(())
}

async fn test2(ctx: &mut Context) -> anyhow::Result<()> {
    println!("2");
    ctx.state.set(Two);
    Ok(())
}

async fn test3(ctx: &mut Context) -> anyhow::Result<()> {
    println!("3");
    ctx.state.set(Three);
    Ok(())
}

async fn test_system(ctx: &mut Context) -> anyhow::Result<()> {
    println!("SYSTEM");
    dbg!(ctx);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_initializer(test1);
    // app.add_initializer(test2);
    // app.add_initializer(test3);
    // app.add_initializer(test_system);

    app.run().await.unwrap();
}
