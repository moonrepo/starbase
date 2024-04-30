use starbase::diagnostics::{Diagnostic, Error, IntoDiagnostic};
use starbase::style::{Style, Stylize};
use starbase::tracing::{debug, info, warn};
use starbase::{subscriber, system, App, Emitter, Event, MainResult, State};
use starbase_utils::{fs, glob};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Diagnostic, Error)]
enum AppError {
    #[error("this {}", "error".style(Style::Success))]
    #[diagnostic(code(oops::my::bad), help("miette error"))]
    Test,
}

#[derive(Debug, State)]
struct TestState(pub String);

#[derive(Debug, Event)]
#[event(dataset = usize)]
struct TestEvent;

#[subscriber]
async fn update_event(mut data: TestEvent) {
    *data = 100;
}

#[system]
async fn start_one(states: States, emitters: Emitters) {
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
            info!("startup 2");
            let _ = states.get::<TestState>();

            // dbg!(state);

            log::info!("This comes from the log crate");
        })
        .await
        .into_diagnostic()?;
    }
}

#[system]
async fn analyze_one(state: StateMut<TestState>, em: EmitterRef<TestEvent>) {
    info!(val = state.0, "analyze {}", "foo.bar".style(Style::File));
    **state = "mutated".to_string();

    let event = TestEvent;
    // dbg!(&event);
    let _data = em.emit(event).await.unwrap();
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

    let _lock = fs::lock_directory(env::current_dir().unwrap().join("foo")).unwrap();

    sleep(Duration::new(10, 0)).await;
}

#[system]
async fn missing_file() {
    fs::read_file(PathBuf::from("fake.file")).unwrap();
}

#[system]
async fn fail() {
    if let Ok(fail) = std::env::var("FAIL") {
        if fail == "panic" {
            panic!("This paniced!");
        }

        warn!("<caution>fail</caution>");
        return Err(AppError::Test)?;
    }
}

#[tokio::main]
async fn main() -> MainResult {
    glob::add_global_negations(["**/target/**"]);

    App::setup_diagnostics();
    App::setup_tracing();

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
