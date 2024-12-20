use super::input_field::*;
use super::Validator;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputProps<'a> {
    pub default_value: String,
    pub description: Option<String>,
    pub label: String,
    pub prefix_symbol: Option<String>,
    pub validate: Validator<'static, String>,
    pub on_value: Option<&'a mut String>,
}

#[component]
pub fn Input<'a>(props: &mut InputProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut value = hooks.use_state(|| props.default_value.clone());
    let mut should_exit = hooks.use_state(|| false);
    let mut error = hooks.use_state(|| None);

    let validate = props.validate.take();

    hooks.use_local_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent {
                code: KeyCode::Enter,
                kind,
                ..
            }) if kind != KeyEventKind::Release => {
                if let Some(msg) = validate(value.to_string()) {
                    error.set(Some(msg));
                    return;
                } else {
                    error.set(None);
                }

                should_exit.set(true);
            }
            _ => {}
        }
    });

    if should_exit.get() {
        if let Some(outer_value) = &mut props.on_value {
            **outer_value = value.to_string();
        }

        system.exit();

        return element! {
            InputFieldValue(
                label: &props.label,
                value: value.read().as_str(),
            )
        }
        .into_any();
    }

    element! {
        InputField(
            label: &props.label,
            description: props.description.clone(),
            error: Some(error),
        ) {
            Box {
                Box(margin_right: 1) {
                    Text(
                        content: props.prefix_symbol.as_ref().unwrap_or(&theme.input_prefix_symbol),
                        color: theme.input_prefix_color,
                    )
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
