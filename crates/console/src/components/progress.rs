use super::layout::Group;
use super::styled_text::StyledText;
use crate::ui::ConsoleTheme;
use flume::{Receiver, Sender};
use iocraft::prelude::*;
use std::time::Duration;

pub enum ProgressState {
    Exit,
    Max(u32),
    Message(String),
    Prefix(String),
    Suffix(String),
    Tick(Option<Duration>),
    Value(u32),
}

#[derive(Clone)]
pub struct ProgressReporter {
    pub tx: Sender<ProgressState>,
    pub rx: Receiver<ProgressState>,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        let (tx, rx) = flume::unbounded::<ProgressState>();

        Self { tx, rx }
    }
}

impl ProgressReporter {
    pub fn exit(&self) {
        self.set(ProgressState::Exit);
    }

    pub fn set(&self, state: ProgressState) {
        let _ = self.tx.send(state);
    }

    pub fn set_max(&self, value: u32) {
        self.set(ProgressState::Max(value));
    }

    pub fn set_message(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Message(value.as_ref().to_owned()));
    }

    pub fn set_prefix(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Prefix(value.as_ref().to_owned()));
    }

    pub fn set_suffix(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Suffix(value.as_ref().to_owned()));
    }

    pub fn set_tick(&self, value: Option<Duration>) {
        self.set(ProgressState::Tick(value));
    }

    pub fn set_value(&self, value: u32) {
        self.set(ProgressState::Value(value));
    }
}

#[derive(Props)]
pub struct ProgressBarProps {
    pub auto_tick: Option<Duration>,
    pub bar_color: Option<Color>,
    pub bar_width: i32,
    pub char_filled: char,
    pub char_position: char,
    pub char_unfilled: char,
    pub default_max: i32,
    pub default_message: String,
    pub default_value: i32,
    pub reporter: ProgressReporter,
    pub tick_step: i32,
    pub tick_loop: bool,
}

impl Default for ProgressBarProps {
    fn default() -> Self {
        Self {
            auto_tick: None,
            bar_color: None,
            bar_width: 30,
            char_filled: '█',
            char_position: '▒',
            char_unfilled: '░',
            default_max: 100,
            default_message: "".into(),
            default_value: 0,
            reporter: Default::default(),
            tick_step: 2,
            tick_loop: false,
        }
    }
}

#[component]
pub fn ProgressBar<'a>(
    props: &mut ProgressBarProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut prefix = hooks.use_state(String::new);
    let mut message = hooks.use_state(|| props.default_message.clone());
    let mut suffix = hooks.use_state(String::new);
    let mut max = hooks.use_state(|| props.default_max as u32);
    let mut value = hooks.use_state(|| props.default_value as u32);
    let mut tick = hooks.use_state(|| props.auto_tick);
    let mut should_exit = hooks.use_state(|| false);

    let receiver = props.reporter.rx.clone();
    let tick_step = props.tick_step as u32;
    let tick_loop = props.tick_loop;

    hooks.use_future(async move {
        loop {
            while let Ok(state) = receiver.recv_async().await {
                match state {
                    ProgressState::Exit => {
                        should_exit.set(true);
                    }
                    ProgressState::Max(val) => {
                        max.set(val);
                    }
                    ProgressState::Message(val) => {
                        message.set(val);
                    }
                    ProgressState::Prefix(val) => {
                        prefix.set(val);
                    }
                    ProgressState::Suffix(val) => {
                        suffix.set(val);
                    }
                    ProgressState::Tick(val) => {
                        tick.set(val);
                    }
                    ProgressState::Value(val) => {
                        if val >= max.get() {
                            value.set(max.get());
                            should_exit.set(true);
                        } else {
                            value.set(val);
                        }
                    }
                };
            }
        }
    });

    hooks.use_future(async move {
        let Some(duration) = tick.get() else {
            return;
        };

        loop {
            tokio::time::sleep(duration).await;

            let next_value = value.get() + tick_step;
            let max_length = max.get();

            if next_value <= max_length {
                value.set(next_value);
            } else if tick_loop {
                value.set(0);
            } else {
                should_exit.set(true);
            }
        }
    });

    let bar_color = props.bar_color.unwrap_or(theme.brand_color);
    let bar_percent = max.get() as f32 * (value.get() as f32 / 100.0);
    let bar_total_width = props.bar_width as u32;
    let bar_filled_width = (bar_total_width as f32 * (bar_percent / 100.0)) as u32;
    let mut bar_unfilled_width = bar_total_width - bar_filled_width;

    // When theres a position to show, we need to reduce the unfilled bar by 1
    if bar_percent > 0.0 && bar_percent < 100.0 {
        bar_unfilled_width -= 1;
    }

    if should_exit.get() {
        system.exit();

        return element!(Box).into_any();
    }

    element! {
        Group(gap: 1) {
            Box(width: Size::Length(bar_total_width)) {
                Text(content:
                    String::from(props.char_filled).repeat(bar_filled_width as usize),
                    color: bar_color,
                )

                #(if bar_percent == 0.0 || bar_percent == 100.0 {
                    None
                } else {
                    Some(element! {
                        Text(
                            content: String::from(props.char_position),
                            color: bar_color,
                        )
                    })
                })

                Text(
                    content: String::from(props.char_unfilled).repeat(bar_unfilled_width as usize),
                    color: bar_color,
                )
            }
            Box {
                StyledText(content: format!("{prefix}{message}{suffix}"))
            }
        }
    }
    .into_any()
}
