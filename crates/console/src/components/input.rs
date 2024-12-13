use super::input_field::*;
use super::Validator;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Props)]
pub struct InputProps<'a> {
    pub default_value: String,
    pub description: Option<String>,
    pub label: String,
    pub on_changed: Handler<'static, String>,
    pub prefix_char: char,
    pub validate: Validator<'static, String>,
    pub value: Option<&'a mut String>,
}

impl Default for InputProps<'_> {
    fn default() -> Self {
        Self {
            default_value: String::new(),
            description: None,
            label: String::new(),
            on_changed: Handler::default(),
            prefix_char: '❯',
            validate: Validator::default(),
            value: None,
        }
    }
}

#[component]
pub fn Input<'a>(props: &mut InputProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut value = hooks.use_state(|| props.default_value.clone());
    let mut submitted = hooks.use_state(|| false);
    let mut should_exit = hooks.use_state(|| false);
    let mut error = hooks.use_state(String::new);

    let validate = props.validate.take();

    hooks.use_local_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => {
                match code {
                    KeyCode::Enter => {
                        if let Some(msg) = validate(value.to_string()) {
                            error.set(msg);
                            return;
                        } else {
                            error.set(String::new());
                        }

                        submitted.set(true);
                        should_exit.set(true);
                    }
                    KeyCode::Esc => {
                        should_exit.set(true);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    if should_exit.get() {
        if submitted.get() {
            if let Some(outer_value) = &mut props.value {
                **outer_value = value.to_string();
            }
        }

        system.exit();

        return element! {
            InputFieldValue(
                label: &props.label,
                value: if submitted.get() {
                    value.to_string()
                } else {
                    String::new()
                }
            )
        }
        .into_any();
    }

    element! {
        InputField(
            label: &props.label,
            description: props.description.clone(),
            error: Some(error.clone()),
        ) {
            Box {
                Box(margin_right: 1) {
                    Text(content: props.prefix_char, color: theme.input_prefix_color)
                }
                Box(width: 50) {
                    TextInput(
                        has_focus: true,
                        value: value.to_string(),
                        on_change: move |new_value| {
                            value.set(new_value);
                        },
                    )
                }
            }
        }
    }
    .into_any()
}
