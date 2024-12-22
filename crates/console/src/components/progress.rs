use super::layout::Group;
use super::styled_text::StyledText;
use crate::ui::ConsoleTheme;
use crate::utils::estimator::Estimator;
use crate::utils::formats::*;
use flume::{Receiver, Sender};
use iocraft::prelude::*;
use std::time::{Duration, Instant};

pub enum ProgressState {
    CustomInt(usize),
    CustomString(String),
    Exit,
    Max(u64),
    Message(String),
    Prefix(String),
    Suffix(String),
    Tick(Duration),
    Value(u64),
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

    pub fn set_max(&self, value: u64) {
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

    pub fn set_tick(&self, value: Duration) {
        self.set(ProgressState::Tick(value));
    }

    pub fn set_value(&self, value: u64) {
        self.set(ProgressState::Value(value));
    }
}

#[derive(Props)]
pub struct ProgressBarProps {
    pub bar_color: Option<Color>,
    pub bar_width: u32,
    pub char_filled: Option<char>,
    pub char_position: Option<char>,
    pub char_unfilled: Option<char>,
    pub default_max: u64,
    pub default_message: String,
    pub default_value: u64,
    pub reporter: ProgressReporter,
}

impl Default for ProgressBarProps {
    fn default() -> Self {
        Self {
            bar_color: None,
            bar_width: 30,
            char_filled: None,
            char_position: None,
            char_unfilled: None,
            default_max: 100,
            default_message: "".into(),
            default_value: 0,
            reporter: Default::default(),
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
    let mut max = hooks.use_state(|| props.default_max);
    let mut value = hooks.use_state(|| props.default_value);
    let mut estimator = hooks.use_state(Estimator::new);
    let mut should_exit = hooks.use_state(|| false);
    let started = hooks.use_state(Instant::now);

    let receiver = props.reporter.rx.clone();

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
                    ProgressState::Value(val) => {
                        if val >= max.get() {
                            value.set(max.get());
                            should_exit.set(true);
                        } else {
                            value.set(val);
                        }
                    }
                    _ => {}
                };
            }
        }
    });

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            estimator.write().record(value.get(), Instant::now());
        }
    });

    let char_filled = props.char_filled.unwrap_or(theme.progress_bar_filled_char);
    let char_unfilled = props
        .char_unfilled
        .unwrap_or(theme.progress_bar_unfilled_char);
    let char_position = props
        .char_position
        .unwrap_or(theme.progress_bar_position_char);
    let bar_color = props.bar_color.unwrap_or(theme.progress_bar_color);
    let bar_percent = calculate_percent(value.get(), max.get());
    let bar_total_width = props.bar_width as u64;
    let bar_filled_width = (bar_total_width as f64 * (bar_percent / 100.0)) as u64;
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
            Box(width: Size::Length(props.bar_width)) {
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
            Box {
                StyledText(
                    content: format!(
                        "{prefix}{}{suffix}",
                        get_message(MessageData {
                            estimator: Some(estimator.read()),
                            max: max.get(),
                            message: message.read(),
                            started: started.get(),
                            value: value.get(),
                        })
                    )
                )
            }
        }
    }
    .into_any()
}

#[derive(Props)]
pub struct ProgressLoaderProps {
    pub loader_color: Option<Color>,
    pub loader_frames: Option<Vec<String>>,
    pub default_message: String,
    pub reporter: ProgressReporter,
    pub tick_interval: Duration,
}

impl Default for ProgressLoaderProps {
    fn default() -> Self {
        Self {
            loader_color: None,
            loader_frames: None,
            default_message: "".into(),
            reporter: Default::default(),
            tick_interval: Duration::from_millis(100),
        }
    }
}

#[component]
pub fn ProgressLoader<'a>(
    props: &mut ProgressLoaderProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut prefix = hooks.use_state(String::new);
    let mut message = hooks.use_state(|| props.default_message.clone());
    let mut suffix = hooks.use_state(String::new);
    let frames = hooks.use_state(|| {
        props
            .loader_frames
            .clone()
            .unwrap_or_else(|| theme.progress_loader_frames.clone())
    });
    let mut frame_index = hooks.use_state(|| 0);
    let mut tick_interval = hooks.use_state(|| props.tick_interval);
    let mut should_exit = hooks.use_state(|| false);
    let started = hooks.use_state(Instant::now);

    let receiver = props.reporter.rx.clone();
    let frames_total = frames.read().len();

    hooks.use_future(async move {
        loop {
            while let Ok(state) = receiver.recv_async().await {
                match state {
                    ProgressState::Exit => {
                        should_exit.set(true);
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
                        tick_interval.set(val);
                    }
                    _ => {}
                };
            }
        }
    });

    hooks.use_future(async move {
        loop {
            tokio::time::sleep(tick_interval.get()).await;
            frame_index.set((frame_index + 1) % frames_total);
        }
    });

    if should_exit.get() {
        system.exit();

        return element!(Box).into_any();
    }

    element! {
        Group(gap: 1) {
            Box {
                Text(
                    content: &frames.read()[frame_index.get()],
                    color: props.loader_color.unwrap_or(theme.progress_loader_color),
                )
            }
            Box {
                StyledText(
                    content: format!(
                        "{prefix}{}{suffix}",
                        get_message(MessageData {
                            estimator: None,
                            max: frames_total as u64,
                            message: message.read(),
                            started: started.get(),
                            value: frame_index.get() as u64,
                        })
                    )
                )
            }
        }
    }
    .into_any()
}

fn calculate_percent(value: u64, max: u64) -> f64 {
    (max as f64 * (value as f64 / 100.0)).clamp(0.0, 100.0)
}

struct MessageData<'a> {
    estimator: Option<StateRef<'a, Estimator>>,
    max: u64,
    message: StateRef<'a, String>,
    started: Instant,
    value: u64,
}

fn get_message(data: MessageData) -> String {
    let mut message = data.message.to_owned();

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

    if let Some(estimator) = data.estimator {
        let eta = estimator.calculate_eta(data.value, data.max);
        let sps = estimator.calculate_sps();

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
    }

    message
}
