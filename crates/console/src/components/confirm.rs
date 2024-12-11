use super::input_field::*;
use super::styled_text::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Props)]
pub struct ConfirmProps {
    pub description: Option<String>,
    pub label: String,
    pub legend: bool,
    pub no_label: String,
    pub no_value: char,
    pub on_confirm: Handler<'static, bool>,
    pub yes_label: String,
    pub yes_value: char,
}

impl Default for ConfirmProps {
    fn default() -> Self {
        Self {
            description: None,
            label: "".into(),
            legend: true,
            no_label: "No".into(),
            no_value: 'n',
            on_confirm: Handler::default(),
            yes_label: "Yes".into(),
            yes_value: 'y',
        }
    }
}

#[component]
pub fn Confirm<'a>(props: &mut ConfirmProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut focused = hooks.use_state(|| 0);
    let mut confirmed = hooks.use_state(|| false);
    let mut should_exit = hooks.use_state(|| false);

    let yes = props.yes_value;
    let no = props.no_value;

    let mut set_focused = move |index: isize| {
        if index > 1 {
            focused.set(0);
        } else if index < 0 {
            focused.set(1);
        } else {
            focused.set(index);
        }
    };

    let mut handle_confirm = move |state: bool| {
        confirmed.set(state);
        should_exit.set(true);
    };

    let mut handle_confirm_via_focus = move || {
        handle_confirm(focused.get() == 0);
    };

    hooks.use_local_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => {
                match code {
                    KeyCode::Char(ch) if ch == yes || ch == no => {
                        handle_confirm(ch == yes);
                    }
                    KeyCode::Esc => {
                        handle_confirm(false);
                    }
                    KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
                        set_focused(focused.get() - 1);
                    }
                    KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                        set_focused(focused.get() + 1);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    if should_exit.get() {
        (props.on_confirm)(confirmed.get());
        system.exit();
    }

    element! {
        InputField(label: &props.label, description: props.description.clone()) {
            Box(margin_top: 1) {
                Button(
                    has_focus: focused == 0,
                    handler: move |_|  {
                        handle_confirm_via_focus();
                    }
                ) {
                    Box(
                        padding_left: 1,
                        padding_right: 1,
                        background_color: if focused == 0 {
                            theme.border_focus_color
                        } else {
                            theme.border_color
                        },
                    ) {
                        StyledText(content: &props.yes_label)
                    }
                }

                Box(width: 1)

                Button(
                    has_focus: focused == 1,
                    handler: move |_|  {
                        handle_confirm_via_focus();
                    }
                ) {
                    Box(
                        padding_left: 1,
                        padding_right: 1,
                        background_color: if focused == 1 {
                            theme.border_focus_color
                        } else {
                            theme.border_color
                        },
                    ) {
                        StyledText(content: &props.no_label)
                    }
                }
            }

            #(props.legend.then(|| {
                element! {
                    Box(margin_top: 1) {
                        StyledText(
                            content: format!("<mutedlight>{yes}/{no}</mutedlight> confirm ⁃ <mutedlight>←/→</mutedlight> toggle ⁃ <mutedlight>ent/spc</mutedlight> select ⁃ <mutedlight>esc</mutedlight> cancel"),
                            style: Style::Muted
                        )
                    }
                }
            }))
        }
    }
}
