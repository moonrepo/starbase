use super::layout::Group;
use super::styled_text::StyledText;
use crate::ui::ConsoleTheme;
use crate::utils::formats::*;
use flume::{Receiver, Sender};
use iocraft::prelude::*;
use std::time::{Duration, Instant};

pub enum ProgressState {
    Exit,
    Max(u32),
    Message(String),
    Prefix(String),
    Suffix(String),
    Tick(Duration),
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

    pub fn set_tick(&self, value: Duration) {
        self.set(ProgressState::Tick(value));
    }

    pub fn set_value(&self, value: u32) {
        self.set(ProgressState::Value(value));
    }
}

#[derive(Props)]
pub struct ProgressBarProps {
    pub bar_color: Option<Color>,
    pub bar_width: i32,
    pub char_filled: Option<char>,
    pub char_position: Option<char>,
    pub char_unfilled: Option<char>,
    pub default_max: i32,
    pub default_message: String,
    pub default_value: i32,
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
    let mut max = hooks.use_state(|| props.default_max as u32);
    let mut value = hooks.use_state(|| props.default_value as u32);
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

    // This purely exists to trigger a re-render so that tokens within the
    // message are dynamically updated with the latest information
    hooks.use_future(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            max.set(max.get());
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
                        get_message(message.read().as_str(), value.get(), max.get(), started.get())
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
    let mut frame_index = hooks.use_state(|| 0);
    let mut tick_interval = hooks.use_state(|| props.tick_interval);
    let mut should_exit = hooks.use_state(|| false);
    let started = hooks.use_state(Instant::now);

    let receiver = props.reporter.rx.clone();
    let frames = props
        .loader_frames
        .clone()
        .unwrap_or_else(|| theme.progress_loader_frames.clone());
    let frames_total = frames.len() as u32;

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
                    content: &frames[frame_index.get() as usize],
                    color: props.loader_color.unwrap_or(theme.progress_loader_color),
                )
            }
            Box {
                StyledText(
                    content: format!(
                        "{prefix}{}{suffix}",
                        get_message(
                            message.read().as_str(),
                            frame_index.get(),
                            frames_total,
                            started.get()
                        )
                    )
                )
            }
        }
    }
    .into_any()
}

fn calculate_percent(value: u32, max: u32) -> f32 {
    (max as f32 * (value as f32 / 100.0)).clamp(0.0, 100.0)
}

// fn calculate_eta(value: u32, max: u32) -> Duration {
//     let steps_per_second = 0.0;

//     if steps_per_second == 0.0 {
//         return Duration::new(0, 0);
//     }

//     let s = max.saturating_sub(value) as f64 / steps_per_second;
//     let secs = s.trunc() as u64;
//     let nanos = (s.fract() * 1_000_000_000f64) as u32;

//     Duration::new(secs, nanos)
// }

// TODO: eta, per_sec, duration
fn get_message(message: &str, value: u32, max: u32, started: Instant) -> String {
    let mut message = message.to_owned();

    if message.contains("{value}") {
        message = message.replace("{value}", &value.to_string());
    }

    if message.contains("{total}") {
        message = message.replace("{total}", &max.to_string());
    }

    if message.contains("{max}") {
        message = message.replace("{max}", &max.to_string());
    }

    if message.contains("{percent}") {
        message = message.replace("{percent}", &calculate_percent(value, max).to_string());
    }

    if message.contains("{bytes}") {
        message = message.replace("{bytes}", &format_bytes_binary(value as u64));
    }

    if message.contains("{total_bytes}") {
        message = message.replace("{total_bytes}", &format_bytes_binary(max as u64));
    }

    if message.contains("{binary_bytes}") {
        message = message.replace("{binary_bytes}", &format_bytes_binary(value as u64));
    }

    if message.contains("{binary_total_bytes}") {
        message = message.replace("{binary_total_bytes}", &format_bytes_binary(max as u64));
    }

    if message.contains("{decimal_bytes}") {
        message = message.replace("{decimal_bytes}", &format_bytes_decimal(value as u64));
    }

    if message.contains("{decimal_total_bytes}") {
        message = message.replace("{decimal_total_bytes}", &format_bytes_decimal(max as u64));
    }

    if message.contains("{elapsed}") {
        message = message.replace("{elapsed}", &format_duration(started.elapsed(), true));
    }

    // if message.contains("{eta}") {
    //     message = message.replace("{eta}", &format_duration(calculate_eta(value, max), true));
    // }

    // if message.contains("{duration}") {
    //     message = message.replace(
    //         "{duration}",
    //         &format_duration(
    //             started.elapsed().saturating_add(calculate_eta(value, max)),
    //             true,
    //         ),
    //     );
    // }

    message
}
