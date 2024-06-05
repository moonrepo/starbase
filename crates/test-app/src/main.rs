#![allow(dead_code)]

use async_trait::async_trait;
use starbase::diagnostics::{Diagnostic, IntoDiagnostic};
use starbase::style::{Style, Stylize};
use starbase::tracing::{info, warn, TracingOptions};
use starbase::{App, AppResult, AppSession, MainResult};
use starbase_utils::{fs, glob};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use tokio::time::sleep;

#[derive(Debug, Diagnostic, Error)]
enum AppError {
    #[error("this {}", "error".style(Style::Success))]
    #[diagnostic(code(oops::my::bad), help("miette error"))]
    Test,
}

#[derive(Clone, Debug, Default)]
struct TestSession {
    state: String,
    active: bool,
}

#[async_trait]
impl AppSession for TestSession {
    async fn startup(&mut self) -> AppResult {
        info!("startup 1");

        self.state = "original".into();
        self.active = true;

        tokio::spawn(async move {
            info!("startup 2");

            log::info!("This comes from the log crate");
        })
        .await
        .into_diagnostic()?;

        Ok(())
    }

    async fn analyze(&mut self) -> AppResult {
        info!(val = self.state, "analyze {}", "foo.bar".style(Style::File));
        self.state = "mutated".into();

        Ok(())
    }

    async fn shutdown(&mut self) -> AppResult {
        info!(val = self.state, "shutdown");

        Ok(())
    }
}

async fn create_file() -> AppResult {
    test_lib::create_file()?;

    let _lock = fs::lock_directory(env::current_dir().unwrap().join("foo")).unwrap();

    sleep(Duration::new(10, 0)).await;

    Ok(())
}

async fn missing_file() -> AppResult {
    fs::read_file(PathBuf::from("fake.file")).unwrap();

    Ok(())
}

async fn fail() -> AppResult {
    if let Ok(fail) = std::env::var("FAIL") {
        if fail == "panic" {
            panic!("This paniced!");
        }

        warn!("<caution>fail</caution>");
        return Err(AppError::Test)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> MainResult {
    glob::add_global_negations(["**/target/**"]);

    let app = App::default();
    app.setup_diagnostics();

    let _guard = app.setup_tracing(TracingOptions {
        log_file: Some(PathBuf::from("test.log")),
        dump_trace: false,
        ..Default::default()
    });

    let mut session = TestSession::default();

    app.run(&mut session, |session| async {
        dbg!(session);
        create_file().await?;
        Ok(())
    })
    .await?;

    Ok(())
}
