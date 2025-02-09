use super::styled_text::StyledText;
use super::OwnedOrShared;
use crate::ui::ConsoleTheme;
use crate::utils::estimator::Estimator;
use crate::utils::formats::*;
use iocraft::prelude::*;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::{self, Receiver, Sender};
use tokio::time::sleep;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProgressDisplay {
    Bar,
    Loader,
}

#[derive(Clone, Debug)]
pub enum ProgressState {
    CustomInt(usize),
    CustomString(String),
    Display(ProgressDisplay),
    Exit,
    Max(u64),
    Message(String),
    Prefix(String),
    Suffix(String),
    Tick(Option<Duration>),
    Value(u64),
    Wait(Duration),
}

#[derive(Clone)]
pub struct ProgressReporter {
    tx: Sender<ProgressState>,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        let (tx, _rx) = broadcast::channel::<ProgressState>(1000);

        Self { tx }
    }
}

impl From<ProgressReporter> for Option<OwnedOrShared<ProgressReporter>> {
    fn from(value: ProgressReporter) -> Self {
        Some(OwnedOrShared::Owned(value))
    }
}

impl ProgressReporter {
    pub fn subscribe(&self) -> Receiver<ProgressState> {
        self.tx.subscribe()
    }

    pub fn exit(&self) -> &Self {
        self.set(ProgressState::Exit)
    }

    pub fn wait(&self, value: Duration) -> &Self {
        self.set(ProgressState::Wait(value))
    }

    pub fn set(&self, state: ProgressState) -> &Self {
        // Will panic if there are no receivers, which can happen
        // while waiting for the components to start rendering!
        let _ = self.tx.send(state);

        self
    }

    pub fn set_display(&self, value: ProgressDisplay) -> &Self {
        self.set(ProgressState::Display(value))
    }

    pub fn set_max(&self, value: u64) -> &Self {
        self.set(ProgressState::Max(value))
    }

    pub fn set_message(&self, value: impl AsRef<str>) -> &Self {
        self.set(ProgressState::Message(value.as_ref().to_owned()))
    }

    pub fn set_prefix(&self, value: impl AsRef<str>) -> &Self {
        self.set(ProgressState::Prefix(value.as_ref().to_owned()))
    }

    pub fn set_suffix(&self, value: impl AsRef<str>) -> &Self {
        self.set(ProgressState::Suffix(value.as_ref().to_owned()))
    }

    pub fn set_tick(&self, value: Option<Duration>) -> &Self {
        self.set(ProgressState::Tick(value))
    }

    pub fn set_value(&self, value: u64) -> &Self {
        self.set(ProgressState::Value(value))
    }
}

#[derive(Props)]
pub struct ProgressProps {
    // Bar
    pub bar_width: u32,
    pub bar_filled_char: Option<char>,
    pub bar_position_char: Option<char>,
    pub bar_unfilled_char: Option<char>,
    // Loader
    pub loader_frames: Option<Vec<String>>,
    pub loader_interval: Option<Duration>,
    // Shared
    pub color: Option<Color>,
    pub default_max: u64,
    pub default_message: String,
    pub default_value: u64,
    pub display: ProgressDisplay,
    pub reporter: Option<OwnedOrShared<ProgressReporter>>,
}

impl Default for ProgressProps {
    fn default() -> Self {
        Self {
            color: None,
            bar_width: 30,
            bar_filled_char: None,
            bar_position_char: None,
            bar_unfilled_char: None,
            loader_frames: None,
            loader_interval: None,
            default_max: 100,
            default_message: "".into(),
            default_value: 0,
            display: ProgressDisplay::Bar,
            reporter: None,
        }
    }
}

#[component]
pub fn Progress<'a>(props: &mut ProgressProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);
    let mut prefix = hooks.use_state(String::new);
    let mut message = hooks.use_state(|| props.default_message.clone());
    let mut suffix = hooks.use_state(String::new);
    let mut max = hooks.use_state(|| props.default_max);
    let mut value = hooks.use_state(|| props.default_value);
    let mut estimator = hooks.use_state(Estimator::new);
    let mut display = hooks.use_state(|| props.display);
    let started = hooks.use_state(Instant::now);

    // Loader
    let frames = hooks.use_state(|| {
        props
            .loader_frames
            .clone()
            .unwrap_or_else(|| theme.progress_loader_frames.clone())
    });
    let mut frame_index = hooks.use_state(|| 0);
    let mut tick_interval = hooks.use_state(|| {
        props.loader_interval.or_else(|| {
            if props.display == ProgressDisplay::Loader {
                Some(Duration::from_millis(100))
            } else {
                None
            }
        })
    });

    let reporter = props.reporter.take();

    hooks.use_future(async move {
        loop {
            let interval = tick_interval.get();

            sleep(interval.unwrap_or(Duration::from_millis(250))).await;

            if interval.is_some() && display.get() == ProgressDisplay::Loader {
                frame_index.set((frame_index + 1) % frames.read().len());
            }

            estimator.write().record(value.get(), Instant::now());
        }
    });

    hooks.use_future(async move {
        let Some(reporter) = reporter else {
            return;
        };

        let mut receiver = reporter.subscribe();

        while let Ok(state) = receiver.recv().await {
            match state {
                ProgressState::Wait(val) => {
                    sleep(val).await;
                }
                ProgressState::Exit => {
                    should_exit.set(true);
                    break;
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
                ProgressState::Value(val) => {
                    value.set(val);
                }
                ProgressState::Tick(val) => {
                    tick_interval.set(val);
                }
                ProgressState::Display(val) => {
                    display.set(val);
                }
                _ => {}
            };
        }
    });

    if should_exit.get() {
        system.exit();

        return element!(View).into_any();
    }

    match display.get() {
        ProgressDisplay::Bar => {
            let char_filled = props
                .bar_filled_char
                .unwrap_or(theme.progress_bar_filled_char);
            let char_unfilled = props
                .bar_unfilled_char
                .unwrap_or(theme.progress_bar_unfilled_char);
            let char_position = props
                .bar_position_char
                .unwrap_or(theme.progress_bar_position_char);
            let bar_color = props.color.unwrap_or(theme.progress_bar_color);
            let bar_percent = calculate_percent(value.get(), max.get());
            let bar_total_width = props.bar_width as u64;
            let bar_filled_width = (bar_total_width as f64 * (bar_percent / 100.0)) as u64;
            let mut bar_unfilled_width = bar_total_width - bar_filled_width;

            // When theres a position to show, we need to reduce the unfilled bar by 1
            if bar_percent > 0.0 && bar_percent < 100.0 {
                bar_unfilled_width -= 1;
            }

            element! {
                View {
                    View(width: Size::Length(props.bar_width), margin_right: 1) {
                        Text(
                            content: String::from(char_filled).repeat(bar_filled_width as usize),
                            color: bar_color,
                        )

                        #(if bar_percent == 0.0 || bar_percent == 100.0 {
                            None
                        } else {
                            Some(element! {
                                Text(
                                    content: String::from(char_position),
                                    color: bar_color,
                                )
                            })
                        })

                        Text(
                            content: String::from(char_unfilled).repeat(bar_unfilled_width as usize),
                            color: bar_color,
                        )
                    }
                    View {
                        StyledText(
                            content: get_message(MessageData {
                                estimator: estimator.read(),
                                max: max.get(),
                                message: format!("{prefix}{message}{suffix}"),
                                started: started.get(),
                                value: value.get(),
                            })
                        )
                    }
                }
            }
            .into_any()
        }
        ProgressDisplay::Loader => element! {
            View {
                View(margin_right: 1) {
                    Text(
                        content: &frames.read()[frame_index.get()],
                        color: props.color.unwrap_or(theme.progress_loader_color),
                    )
                }
                View {
                    StyledText(
                        content: get_message(MessageData {
                            estimator: estimator.read(),
                            max: frames.read().len() as u64,
                            message: format!("{prefix}{message}{suffix}"),
                            started: started.get(),
                            value: frame_index.get() as u64,
                        })
                    )
                }
            }
        }
        .into_any(),
    }
}

fn calculate_percent(value: u64, max: u64) -> f64 {
    (max as f64 * (value as f64 / 100.0)).clamp(0.0, 100.0)
}

struct MessageData<'a> {
    estimator: StateRef<'a, Estimator>,
    max: u64,
    message: String,
    started: Instant,
    value: u64,
}

fn get_message(data: MessageData) -> String {
    let mut message = data.message;

    if message.contains("{value}") {
        message = message.replace("{value}", &data.value.to_string());
    }

    if message.contains("{total}") {
        message = message.replace("{total}", &data.max.to_string());
    }

    if message.contains("{max}") {
        message = message.replace("{max}", &data.max.to_string());
    }

    if message.contains("{percent}") {
        message = message.replace(
            "{percent}",
            &format_float(calculate_percent(data.value, data.max)),
        );
    }

    if message.contains("{bytes}") {
        message = message.replace("{bytes}", &format_bytes_binary(data.value));
    }

    if message.contains("{total_bytes}") {
        message = message.replace("{total_bytes}", &format_bytes_binary(data.max));
    }

    if message.contains("{binary_bytes}") {
        message = message.replace("{binary_bytes}", &format_bytes_binary(data.value));
    }

    if message.contains("{binary_total_bytes}") {
        message = message.replace("{binary_total_bytes}", &format_bytes_binary(data.max));
    }

    if message.contains("{decimal_bytes}") {
        message = message.replace("{decimal_bytes}", &format_bytes_decimal(data.value));
    }

    if message.contains("{decimal_total_bytes}") {
        message = message.replace("{decimal_total_bytes}", &format_bytes_decimal(data.max));
    }

    if message.contains("{elapsed}") {
        message = message.replace("{elapsed}", &format_duration(data.started.elapsed(), true));
    }

    let eta = data.estimator.calculate_eta(data.value, data.max);
    let sps = data.estimator.calculate_sps();

    if message.contains("{eta}") {
        message = message.replace("{eta}", &format_duration(eta, true));
    }

    if message.contains("{duration}") {
        message = message.replace(
            "{duration}",
            &format_duration(data.started.elapsed().saturating_add(eta), true),
        );
    }

    if message.contains("{per_sec}") {
        message = message.replace("{per_sec}", &format!("{:.1}/s", sps));
    }

    if message.contains("{bytes_per_sec}") {
        message = message.replace(
            "{bytes_per_sec}",
            &format!("{}/s", format_bytes_binary(sps as u64)),
        );
    }

    if message.contains("{binary_bytes_per_sec}") {
        message = message.replace(
            "{binary_bytes_per_sec}",
            &format!("{}/s", format_bytes_binary(sps as u64)),
        );
    }

    if message.contains("{decimal_bytes_per_sec}") {
        message = message.replace(
            "{decimal_bytes_per_sec}",
            &format!("{}/s", format_bytes_decimal(sps as u64)),
        );
    }

    message
}
