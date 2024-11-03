#![allow(dead_code)]

use async_trait::async_trait;
use iocraft::prelude::*;
use starbase::{App, AppSession, MainResult};
use starbase_console::ui::*;
use starbase_console::{Console, EmptyReporter};
use std::process::ExitCode;

#[derive(Clone, Debug)]
struct TestSession {
    console: Console<EmptyReporter>,
}

#[async_trait]
impl AppSession for TestSession {}

fn render(session: TestSession, ui: String) {
    let con = &session.console;

    match ui.as_str() {
        "section" => {
            con.render(element! {
                Container {
                    Section(title: "Title")
                    Section(title: "Title") {
                        Text(content: "With content")
                    }
                    Section(title: "Title", title_color: Color::Red) {
                        Text(content: "With colored header")
                    }
                }
            })
            .unwrap();
        }
        _ => panic!("Unknown UI {}.", ui),
    }
}

#[tokio::main]
async fn main() -> MainResult {
    let app = App::default();
    app.setup_diagnostics();
    app.setup_tracing_with_defaults();

    let args = std::env::args().collect::<Vec<_>>();
    let ui = args.get(1).cloned().expect("Missing UI argument!");

    let code = app
        .run(
            TestSession {
                console: Console::new(false),
            },
            |session| async move {
                render(session, ui);
                Ok(None)
            },
        )
        .await?;

    Ok(ExitCode::from(code))
}
