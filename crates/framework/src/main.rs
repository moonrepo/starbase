use core::future::Future;
use starship::{App, Context, ContextManager};
use std::{thread::sleep, time::Duration};

struct One;
struct Two;
struct Three;

async fn test1(context: Context) -> anyhow::Result<()> {
    println!("1");
    // context.write().await.state.set(One);
    Ok(())
}

async fn test2(context: Context) -> anyhow::Result<()> {
    println!("2");
    // context.write().await.state.set(Two);
    Ok(())
}

async fn test3(context: Context) -> anyhow::Result<()> {
    println!("3");
    // context.write().await.state.set(Three);
    Ok(())
}

async fn test_system(context: Context) -> anyhow::Result<()> {
    println!("SYSTEM");
    // dbg!(context.read().await);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_initializer(test1);
    app.add_initializer(test2);
    app.add_initializer(test3);
    app.add_initializer(test_system);

    app.run().await.unwrap();
}
