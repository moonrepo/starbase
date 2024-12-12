use super::input_field::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputProps {
    pub default_value: String,
    pub description: Option<String>,
    pub label: String,
    pub on_changed: Handler<'static, String>,
}

#[component]
pub fn Input<'a>(props: &'a mut InputProps, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut value = hooks.use_state(|| props.default_value.clone());
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_local_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => {
                match code {
                    KeyCode::Enter => {
                        should_exit.set(true);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    if should_exit.get() {
        (props.on_changed)(value.to_string());
        system.exit();

        return element!(Box).into_any();
    }

    element! {
        InputField(
            label: props.label.as_str(),
            description: props.description.as_deref()
        ) {
            TextInput(
                value: value.to_string(),
                on_change: move |new_value| {
                    value.set(new_value);
                },
            )
        }
    }
    .into_any()
}
