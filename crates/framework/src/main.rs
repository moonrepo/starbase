use starship::{App, Context};

fn test_system(ctx: &mut Context) -> anyhow::Result<()> {
    println!("SYSTEM");
    dbg!(ctx);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_initializer(test_system);

    app.run().await.unwrap();
}
