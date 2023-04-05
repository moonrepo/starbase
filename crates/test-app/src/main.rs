use starship::diagnose::{Diagnostic, Error, IntoDiagnostic};
use starship::trace::{debug, info, warn};
use starship::{system, App, MainResult, State};

#[derive(Debug, Diagnostic, Error)]
enum AppError {
    #[error("this error")]
    #[diagnostic(code(oops::my::bad), help("miette error"))]
    Test,
}

#[derive(Debug, State)]
struct Test(pub String);

#[system]
async fn start_one(states: StatesMut) {
    info!("startup 1");
    states.set(Test("original".into()));
    debug!("startup 1");
}

#[system]
async fn start_two(states: States, _resources: Resources, _emitters: Emitters) {
    tokio::spawn(async move {
        let states = states.read().await;
        info!("startup 2");
        let state = states.get::<Test>();
        dbg!(state);
    })
    .await
    .into_diagnostic()?;
}

#[system]
async fn analyze_one(state: StateMut<Test>) {
    info!(val = state.0, "analyze");
    **state = "mutated".to_string();
}

#[system]
async fn finish(state: StateRef<Test>) {
    info!(val = state.0, "shutdown");
    dbg!(state);
}

#[system]
async fn fail() {
    if std::env::var("FAIL").is_ok() {
        warn!("fail");
        return Err(AppError::Test)?;
    }
}

#[tokio::main]
async fn main() -> MainResult {
    App::setup_hooks();

    let mut app = App::new();
    app.shutdown(finish);
    app.analyze(analyze_one);
    app.startup(start_one);
    app.startup(start_two);
    app.execute(fail);

    let ctx = app.run().await?;
    dbg!(ctx);

    Ok(())
}
