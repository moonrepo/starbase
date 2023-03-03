use starship::App;

fn test_system() -> anyhow::Result<()> {
    println!("SYSTEM");

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::new();
    app.add_initializer(test_system);

    app.run().await.unwrap();
}
