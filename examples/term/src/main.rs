#![allow(dead_code)]

use async_trait::async_trait;
use iocraft::prelude::*;
use starbase::{App, AppSession, MainResult};
use starbase_console::ui::*;
use starbase_console::{Console, EmptyReporter};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Clone, Debug)]
struct TestSession {
    console: Console<EmptyReporter>,
}

#[async_trait]
impl AppSession for TestSession {}

async fn render(session: TestSession, ui: String) {
    let con = &session.console;

    match ui.as_str() {
        "confirm" => {
            let mut value = false;

            con.render_interactive(element! {
                Confirm(
                    label: "Are you sure?",
                    description: "This operation cannot be undone!".to_owned(),
                    on_confirm: &mut value
                )
            })
            .await
            .unwrap();
        }
        "entry" => {
            con.render(element! {
                Container {
                    Section(title: "Simple values") {
                        Entry(name: "No content")
                        Entry(
                            name: "Basic content",
                            content: "Value".to_owned(),
                        )
                        Entry(
                            name: "Styled content",
                            value: element! { StyledText(content: "identifier", style: Style::Id) }.into_any()
                        )
                        Entry(
                            name: "Custom separator",
                            value: element! { Text(content: "Value") }.into_any(),
                            separator: " =".to_owned()
                        )
                    }
                    Section(title: "Complex values") {
                        Entry(name: "List") {
                            List {
                                ListItem {
                                    Text(content: "One")
                                }
                                ListItem {
                                    Text(content: "Two")
                                }
                                ListItem {
                                    Text(content: "Three")
                                }
                            }
                        }
                        Entry(name: "Entry") {
                            Entry(name: "No content")
                            Entry(
                                name: "Basic content",
                                value: element! { Text(content: "Value") }.into_any()
                            )
                            Entry(name: "Nested content") {
                                Entry(
                                    name: "Styled content",
                                    value: element! { StyledText(content: "identifier", style: Style::Id) }.into_any()
                                )
                            }
                        }
                    }
                    Section(title: "Composed values") {
                        Entry(
                            name: "Content and children",
                            value: element! { StyledText(content: "3 items", style: Style::MutedLight) }.into_any()
                        ) {
                            List {
                                ListItem {
                                    Text(content: "One")
                                }
                                ListItem {
                                    Text(content: "Two")
                                }
                                ListItem {
                                    Text(content: "Three")
                                }
                            }
                        }
                    }
                }
            })
            .unwrap();
        }
        "input" => {
            let mut value = String::new();

            con.render_interactive(element! {
                Input(
                    label: "What is your name?",
                    on_value: &mut value,
                    validate: |new_value: String| {
                        if new_value.is_empty() {
                            Some("Field is required".into())
                        } else {
                            None
                        }
                    }
                )
            })
            .await
            .unwrap();
        }
        "list" => {
            con.render(element! {
                Container {
                    Section(title: "Default") {
                        List {
                            ListItem {
                                Text(content: "One")
                            }
                            ListItem {
                                Text(content: "Two")
                            }
                            ListItem {
                                Text(content: "Three")
                            }
                        }
                    }
                    Section(title: "Custom bullets") {
                        List {
                            ListItem(bullet: ">>".to_owned()) {
                                Text(content: "One")
                            }
                            ListItem(bullet: ">>".to_owned()) {
                                Text(content: "Two")
                            }
                            ListItem(bullet: ">>".to_owned()) {
                                Text(content: "Three")
                            }
                        }
                    }
                }
            })
            .unwrap();
        }
        "notice" => {
            con.render(element! {
                Container {
                    Notice {
                        Text(content: "Default")
                    }
                    Notice(title: "Title".to_owned()) {
                        Text(content: "With title")
                    }
                    Notice(variant: Variant::Success) {
                        Text(content: "Success state")
                    }
                    Notice(variant: Variant::Success, no_title: true) {
                        Text(content: "Success state without title")
                    }
                    Notice(variant: Variant::Failure) {
                        Text(content: "Failure state")
                    }
                    Notice(variant: Variant::Info) {
                        Text(content: "Info state")
                    }
                    Notice(variant: Variant::Caution) {
                        Text(content: "Caution state")
                    }
                }
            })
            .unwrap();
        }
        "progressbar" => {
            con.render_loop(element! {
                Container {
                    ProgressBar(
                        default_message: "Unfilled - {elapsed} - {duration} - {eta}".to_owned()
                    )
                    ProgressBar(
                        bar_color: Color::Cyan,
                        default_message: "Filled - {bytes}/{total_bytes} - {decimal_bytes}/{decimal_total_bytes}".to_owned(),
                        default_max: 5432,
                        default_value: 5432
                    )
                    ProgressBar(
                        bar_color: Color::Red,
                        char_filled: '━',
                        char_position: '╾',
                        char_unfilled: '─',
                        default_message: "Partially filled with custom bar - {percent}%".to_owned(),
                        default_value: 53
                    )
                }
            })
            .await
            .unwrap();
        }
        "progressreporter" => {
            let reporter = ProgressReporter::default();
            let reporter_clone = reporter.clone();

            tokio::task::spawn(async move {
                let mut count = 0;

                loop {
                    if count >= 100 {
                        break;
                    } else if count == 50 {
                        reporter_clone.set_message(
                            "Loading {value}/{max} ({per_sec}) - {elapsed} - {duration} - {eta}",
                        );
                    } else if count == 25 {
                        reporter_clone.set_prefix("[prefix] ");
                    } else if count == 75 {
                        reporter_clone.set_suffix(" [suffix]");
                    }

                    tokio::time::sleep(Duration::from_millis(250)).await;

                    count += 1;
                    reporter_clone.set_value(count);
                }
            });

            con.render_loop(element! {
                Container {
                    ProgressBar(
                        default_message: "Loading {value}/{max} ({per_sec})".to_owned(),
                        reporter
                    )
                }
            })
            .await
            .unwrap();
        }
        "progressloader" => {
            con.render_loop(element! {
                Container {
                    ProgressLoader(
                        default_message: "Default - {elapsed}".to_owned()
                    )
                    ProgressLoader(
                        default_message: "Custom frames".to_owned(),
                        loader_color: Color::Yellow,
                        loader_frames: vec![
                            "∙∙∙".into(),
                            "●∙∙".into(),
                            "∙●∙".into(),
                            "∙∙●".into(),
                            "∙∙∙".into(),
                        ],
                        tick_interval: Duration::from_millis(125)
                    )
                }
            })
            .await
            .unwrap();
        }
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
        "select" => {
            let mut index = 0usize;

            con.render_interactive(element! {
                Select(
                    default_index: 2,
                    label: "What is your favorite color?",
                    description: "Only choose 1 value.".to_owned(),
                    on_index: &mut index,
                    options: vec![
                        SelectOption::new("red"),
                        SelectOption::new("blue").label("Blue").disabled(),
                        SelectOption::new("green"),
                        SelectOption::new("yellow").disabled(),
                        SelectOption::new("pink").label("Pink"),
                    ]
                )
            })
            .await
            .unwrap();
        }
        "selectmulti" => {
            let mut indexes = vec![];

            con.render_interactive(element! {
                Select(
                    default_indexes: vec![2, 4],
                    label: "What is your favorite color?",
                    description: "Can choose multiple values.".to_owned(),
                    multiple: true,
                    on_indexes: &mut indexes,
                    options: vec![
                        SelectOption::new("red"),
                        SelectOption::new("blue").label("Blue").disabled(),
                        SelectOption::new("green"),
                        SelectOption::new("yellow").disabled(),
                        SelectOption::new("pink").label("Pink"),
                        SelectOption::new("black"),
                        SelectOption::new("white"),
                    ]
                )
            })
            .await
            .unwrap();
        }
        "styledtext" => {
            con.render(element! {
                Container {
                    StyledText(content: "Unstyled")
                    StyledText(content: "Styled success", style: Style::Success)
                    StyledText(content: "Styled failure with weight", style: Style::Failure, weight: Weight::Bold)
                    StyledText(content: "Styled file with decoration", style: Style::File, decoration: TextDecoration::Underline)
                    StyledText(content: "Styled <file>with</file> <path>tags</path>")
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
                render(session, ui).await;
                Ok(None)
            },
        )
        .await?;

    Ok(ExitCode::from(code))
}
