use starbase::diagnostics::{Diagnostic, Error, IntoDiagnostic};
use starbase::style::{Style, Stylize};
use starbase::tracing::{debug, info, warn, TracingOptions};
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

#[derive(Debug, State)]
struct TestState2(pub bool);

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
    states.set(TestState2(true));
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

// HANGS!
// #[system]
// async fn read_write(state1: StateRef<TestState>, state2: StateMut<TestState2>) {
//     {
//         state2.0 = false;
//     }

//     dbg!(&state1);
// }

// HANGS!
// #[system]
// async fn write_write(state1: StateMut<TestState>, state2: StateMut<TestState2>) {
//     {
//         state1.0 = "updated".into();
//     }

//     {
//         state2.0 = false;
//     }
// }

// SOMETIMES HANGS!
#[system]
async fn raw_write_write(state1: StateRaw<TestState>, state2: StateRaw<TestState2>) {
    dbg!(&state1);
    {
        state1.write().0 = "updated".into();
    }
    dbg!(&state1);

    dbg!(&state2);
    {
        state2.write().0 = false;
    }
    dbg!(&state2);
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

    let _guard = App::setup_tracing_with_options(TracingOptions {
        log_file: Some(PathBuf::from("test.log")),
        dump_trace: true,
        ..Default::default()
    });

    let mut app = App::new();
    app.shutdown(finish);
    app.analyze(analyze_one);
    app.startup(start_one);
    app.startup(sub_mod::start_two);
    // app.execute(missing_file);
    // app.execute(read_write);
    // app.execute(write_write);
    // app.execute(raw_write_write);
    // app.execute(create_file);
    app.execute(fail);

    let ctx = app.run().await?;
    dbg!(ctx);

    Ok(())
}
