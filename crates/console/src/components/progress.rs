use super::layout::Group;
use super::styled_text::StyledText;
use crate::ui::ConsoleTheme;
use flume::{Receiver, Sender};
use iocraft::prelude::*;

pub enum ProgressState {
    Length(usize),
    Message(String),
    Position(usize),
    Prefix(String),
    Suffix(String),
}

#[derive(Clone)]
pub struct ProgressReporter {
    tx: Sender<ProgressState>,
    rx: Receiver<ProgressState>,
}

impl Default for ProgressReporter {
    fn default() -> Self {
        let (tx, rx) = flume::unbounded::<ProgressState>();

        Self { tx, rx }
    }
}

impl ProgressReporter {
    pub fn set(&self, state: ProgressState) {
        let _ = self.tx.send(state);
    }

    pub fn set_length(&self, value: usize) {
        self.set(ProgressState::Length(value));
    }

    pub fn set_message(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Message(value.as_ref().to_owned()));
    }

    pub fn set_position(&self, value: usize) {
        self.set(ProgressState::Position(value));
    }

    pub fn set_prefix(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Prefix(value.as_ref().to_owned()));
    }

    pub fn set_suffix(&self, value: impl AsRef<str>) {
        self.set(ProgressState::Suffix(value.as_ref().to_owned()));
    }

    pub fn wait_for_state_changes(&mut self) -> Receiver<ProgressState> {
        self.rx.clone()
    }
}

#[derive(Props)]
pub struct ProgressBarProps {
    pub bar_color: Option<Color>,
    pub bar_width: u32,
    pub char_filled: char,
    pub char_position: char,
    pub char_unfilled: char,
    pub default_length: usize,
    pub default_message: String,
    pub default_position: usize,
    pub reporter: ProgressReporter,
}

impl Default for ProgressBarProps {
    fn default() -> Self {
        Self {
            bar_color: None,
            bar_width: 30,
            char_filled: '█',
            char_position: '▒',
            char_unfilled: '░',
            default_length: 100,
            default_message: "".into(),
            default_position: 0,
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
    let mut length = hooks.use_state(|| props.default_length);
    let mut position = hooks.use_state(|| props.default_position);
    let mut should_exit = hooks.use_state(|| false);

    let receiver = props.reporter.wait_for_state_changes();

    hooks.use_future(async move {
        loop {
            while let Ok(state) = receiver.recv_async().await {
                match state {
                    ProgressState::Length(value) => {
                        length.set(value);
                    }
                    ProgressState::Message(value) => {
                        message.set(value);
                    }
                    ProgressState::Position(value) => {
                        position.set(value);

                        if value == length.get() {
                            should_exit.set(true);
                        }
                    }
                    ProgressState::Prefix(value) => {
                        prefix.set(value);
                    }
                    ProgressState::Suffix(value) => {
                        suffix.set(value);
                    }
                };
            }
        }
    });

    let bar_color = props.bar_color.unwrap_or(theme.brand_color);
    let bar_percent = (length.get() as f32 * (position.get() as f32 / 100.0)) / 100.0;
    let bar_total_width = props.bar_width;
    let bar_filled_width = bar_total_width as f32 * bar_percent;
    let mut bar_unfilled_width = bar_total_width as f32 - bar_filled_width;

    // When theres a position to show, we need to reduce the unfilled bar by 1
    if bar_percent > 0.0 && bar_percent < 1.0 {
        bar_unfilled_width -= 1.0;
    }

    if should_exit.get() {
        system.exit();
    }

    element! {
        Group(gap: 1) {
            Box(width: Size::Length(bar_total_width)) {
                Text(content:
                    String::from(props.char_filled).repeat(bar_filled_width as usize),
                    color: bar_color,
                )

                #(if bar_percent == 0.0 || bar_percent == 1.0 {
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
}
