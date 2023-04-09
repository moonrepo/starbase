use starbase::diagnose::{Diagnostic, Error, IntoDiagnostic};
use starbase::trace::{debug, info, warn};
use starbase::{subscriber, system, App, Emitter, Event, MainResult, State};
use starbase_styles::{Style, Stylize};
use starbase_utils::fs;
use std::path::PathBuf;

#[derive(Debug, Diagnostic, Error)]
enum AppError {
    #[error("this {}", "error".style(Style::Success))]
    #[diagnostic(code(oops::my::bad), help("miette error"))]
    Test,
}

#[derive(Debug, State)]
struct TestState(pub String);

#[derive(Debug, Event)]
struct TestEvent(pub usize);

#[subscriber]
async fn update_event(mut event: TestEvent) {
    event.0 = 100;
}

#[system]
async fn start_one(states: StatesMut, emitters: EmittersMut) {
    info!("startup 1");
    states.set(TestState("original".into()));
    emitters.set(Emitter::<TestEvent>::new());
    debug!("startup 1");
}

mod sub_mod {
    use super::*;

    #[system]
    pub async fn start_two(states: States, _resources: Resources, em: EmitterMut<TestEvent>) {
        em.on(update_event).await;

        tokio::spawn(async move {
            let states = states.read().await;
            info!("startup 2");
            let _ = states.get::<TestState>();

            // dbg!(state);
        })
        .await
        .into_diagnostic()?;
    }
}

#[system]
async fn analyze_one(state: StateMut<TestState>, em: EmitterRef<TestEvent>) {
    info!(val = state.0, "analyze {}", "foo.bar".style(Style::File));
    **state = "mutated".to_string();

    let event = TestEvent(50);
    // dbg!(&event);
    let (_, _) = em.emit(event).await.unwrap();
    // dbg!(event);
}

#[system]
async fn finish(state: StateRef<TestState>) {
    info!(val = state.0, "shutdown");
    // dbg!(state);
}

#[system]
async fn create_file() {
    test_lib::create_file()?;
}

#[system]
async fn missing_file() {
    fs::read_file(PathBuf::from("fake.file"))?;
}

#[system]
async fn fail() {
    if let Ok(fail) = std::env::var("FAIL") {
        if fail == "panic" {
            panic!("This paniced!");
        }

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
    app.startup(sub_mod::start_two);
    // app.execute(missing_file);
    app.execute(create_file);
    app.execute(fail);

    let ctx = app.run().await?;
    dbg!(ctx);

    Ok(())
}
