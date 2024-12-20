use super::input_field::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Clone, Default)]
pub struct SelectOption {
    pub disabled: bool,
    pub label: String,
    pub value: String,
}

impl SelectOption {
    pub fn new(value: impl AsRef<str>) -> Self {
        let value = value.as_ref();

        Self {
            disabled: false,
            label: value.to_owned(),
            value: value.to_owned(),
        }
    }

    pub fn disabled(self) -> Self {
        Self {
            disabled: true,
            ..self
        }
    }

    pub fn label(self, label: impl AsRef<str>) -> Self {
        Self {
            label: label.as_ref().to_owned(),
            ..self
        }
    }
}

#[derive(Props)]
pub struct SelectProps<'a> {
    pub default_index: Option<u32>,
    pub description: Option<String>,
    pub label: String,
    pub legend: bool,
    pub options: Vec<SelectOption>,
    pub prefix_symbol: Option<String>,
    pub selected_symbol: Option<String>,
    pub value: Option<&'a mut usize>,
}

impl Default for SelectProps<'_> {
    fn default() -> Self {
        Self {
            default_index: None,
            description: None,
            label: "".into(),
            legend: true,
            options: vec![],
            prefix_symbol: None,
            selected_symbol: None,
            value: None,
        }
    }
}

#[component]
pub fn Select<'a>(props: &mut SelectProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut active_index = hooks.use_state(|| 0);
    let mut selected_index = hooks.use_state(|| props.default_index.map(|i| i as usize));
    let mut submitted = hooks.use_state(|| false);
    let mut should_exit = hooks.use_state(|| false);
    let mut error = hooks.use_state(|| None);

    let options = props.options.clone();
    let option_last_index = options.len() - 1;

    let get_next_index = move |current: usize, step: isize| -> usize {
        let next = current as isize - step;

        if next < 0 {
            option_last_index
        } else if next > option_last_index as isize {
            0
        } else {
            next as usize
        }
    };

    hooks.use_local_terminal_events({
        move |event| match event {
            TerminalEvent::Key(KeyEvent { code, kind, .. }) if kind != KeyEventKind::Release => {
                error.set(None);

                match code {
                    KeyCode::Char(' ') => {
                        if selected_index
                            .get()
                            .is_some_and(|i| i == active_index.get())
                        {
                            selected_index.set(None);
                        } else {
                            selected_index.set(Some(active_index.get()));
                        }
                    }
                    KeyCode::Enter => {
                        if selected_index.read().is_none() {
                            error.set(Some("Please select an option".into()));
                        } else {
                            submitted.set(true);
                            should_exit.set(true);
                        }
                    }
                    KeyCode::Esc => {
                        should_exit.set(true);
                    }
                    KeyCode::Left | KeyCode::Up => {
                        let mut next_index = match code {
                            KeyCode::Left => 0,
                            KeyCode::Up => get_next_index(active_index.get(), 1),
                            _ => unimplemented!(),
                        };

                        while options.get(next_index).is_some_and(|opt| opt.disabled) {
                            next_index = get_next_index(next_index, 1);
                        }

                        active_index.set(next_index);
                    }
                    KeyCode::Right | KeyCode::Down => {
                        let mut next_index = match code {
                            KeyCode::Right => option_last_index,
                            KeyCode::Down => get_next_index(active_index.get(), -1),
                            _ => unimplemented!(),
                        };

                        while options.get(next_index).is_some_and(|opt| opt.disabled) {
                            next_index = get_next_index(next_index, -1);
                        }

                        active_index.set(next_index);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    if should_exit.get() {
        if submitted.get() {
            if let (Some(outer_value), Some(index)) = (&mut props.value, selected_index.get()) {
                **outer_value = index;
            }
        }

        system.exit();

        return element! {
            InputFieldValue(
                label: &props.label,
                value: if submitted.get() {
                    selected_index.read()
                        .and_then(|index| props.options.get(index))
                        .map(|opt| opt.value.to_owned())
                        .unwrap_or_else(String::new)
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
            error: Some(error),
            footer: props.legend.then(|| {
                element! {
                    InputLegend(legend: vec![
                        ("⎵".into(), "select".into()),
                        ("↕".into(), "move".into()),
                        ("↵".into(), "submit".into()),
                        ("⊘".into(), "cancel".into()),
                    ])
                }.into_any()
            })
        ) {
            Box(flex_direction: FlexDirection::Column, margin_top: 1, margin_bottom: 1) {
                #(props.options.iter().enumerate().map(|(index, opt)| {
                    let active = active_index.get() == index;
                    let selected = selected_index.read().is_some_and(|i| i == index);

                    element! {
                        Box {
                            Box(margin_right: 1) {
                                #(if active {
                                    element! {
                                        Text(
                                            content: props.prefix_symbol.as_ref()
                                                .unwrap_or(&theme.input_prefix_symbol),
                                            color: theme.input_prefix_color,
                                        )
                                    }
                                } else if selected {
                                    element! {
                                        Text(
                                            content: props.selected_symbol.as_ref()
                                                .unwrap_or(&theme.input_selected_symbol),
                                            color: theme.input_selected_color
                                        )
                                    }
                                } else {
                                    element! {
                                        Text(content: " ")
                                    }
                                })
                            }
                            Box {
                                Text(
                                    content: &opt.label,
                                    color: if opt.disabled {
                                        Some(theme.style_muted_light_color)
                                    } else if selected {
                                        Some(theme.input_selected_color)
                                    } else if active {
                                        Some(theme.input_active_color)
                                    } else {
                                        None
                                    }
                                )
                            }
                        }
                    }
                }))
            }
        }
    }
    .into_any()
}
