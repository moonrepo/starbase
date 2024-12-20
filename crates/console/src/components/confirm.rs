use super::input_field::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Props)]
pub struct ConfirmProps<'a> {
    pub description: Option<String>,
    pub label: String,
    pub legend: bool,
    pub no_label: String,
    pub no_char: char,
    pub yes_label: String,
    pub yes_char: char,
    pub on_confirm: Option<&'a mut bool>,
}

impl Default for ConfirmProps<'_> {
    fn default() -> Self {
        Self {
            description: None,
            label: "".into(),
            legend: true,
            no_label: "No".into(),
            no_char: 'n',
            yes_label: "Yes".into(),
            yes_char: 'y',
            on_confirm: None,
        }
    }
}

#[component]
pub fn Confirm<'a>(props: &mut ConfirmProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut focused = hooks.use_state(|| 0);
    let mut confirmed = hooks.use_state(|| false);
    let mut should_exit = hooks.use_state(|| false);
    let mut error = hooks.use_state(|| None);

    let yes = props.yes_char;
    let no = props.no_char;

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
                error.set(None);

                match code {
                    KeyCode::Char(ch) => {
                        if ch == yes || ch == no {
                            handle_confirm(ch == yes);
                        } else {
                            error.set(Some(format!("Please press [{yes}] or [{no}] to confirm")));
                        }
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
        if let Some(outer_value) = &mut props.on_confirm {
            **outer_value = confirmed.get();
        }

        system.exit();

        return element! {
            InputFieldValue(
                label: &props.label,
                value: confirmed.to_string()
            )
        }
        .into_any();
    }

    element! {
        InputField(
            label: &props.label,
            description: props.description.clone(),
            error: Some(error),
            footer: props.legend.then(|| {
                element! {
                    InputLegend(legend: vec![
                        (format!("{yes}/{no}"), "confirm".into()),
                        ("↔".into(), "toggle".into()),
                        ("↵".into(), "submit".into()),
                    ])
                }.into_any()
            })
        ) {
            Box(margin_top: 1, margin_bottom: 1) {
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
                        Text(content: &props.yes_label)
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
                        Text(content: &props.no_label)
                    }
                }
            }
        }
    }
    .into_any()
}
