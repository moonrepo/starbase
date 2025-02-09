use super::input_field::*;
use super::layout::Group;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;
use std::collections::HashSet;

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
    pub default_index: Option<usize>,
    pub default_indexes: Vec<usize>,
    pub description: Option<String>,
    pub label: String,
    pub legend: bool,
    pub multiple: bool,
    pub options: Vec<SelectOption>,
    pub prefix_symbol: Option<String>,
    pub selected_symbol: Option<String>,
    pub on_index: Option<&'a mut usize>,
    pub on_indexes: Option<&'a mut Vec<usize>>,
}

impl Default for SelectProps<'_> {
    fn default() -> Self {
        Self {
            default_index: None,
            default_indexes: vec![],
            description: None,
            label: "".into(),
            legend: true,
            multiple: false,
            options: vec![],
            prefix_symbol: None,
            selected_symbol: None,
            on_index: None,
            on_indexes: None,
        }
    }
}

#[component]
pub fn Select<'a>(props: &mut SelectProps<'a>, mut hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let mut system = hooks.use_context_mut::<SystemContext>();
    let options = hooks.use_state(|| props.options.clone());
    let mut active_index = hooks.use_state(|| props.default_index.unwrap_or_default());
    let mut selected_index = hooks.use_state(|| {
        HashSet::<usize>::from_iter(if props.multiple {
            props.default_indexes.clone()
        } else {
            props
                .default_index
                .map(|index| vec![index])
                .unwrap_or_default()
        })
    });
    let mut should_exit = hooks.use_state(|| false);
    let mut error = hooks.use_state(|| None);

    let multiple = props.multiple;
    let option_last_index = options.read().len() - 1;

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
                        let index = active_index.get();

                        if selected_index.read().contains(&index) {
                            selected_index.write().remove(&index);
                        } else {
                            if !multiple {
                                selected_index.write().clear();
                            }
                            selected_index.write().insert(index);
                        }
                    }
                    KeyCode::Enter => {
                        if selected_index.read().is_empty() {
                            error.set(Some("Please select an option".into()));
                        } else {
                            should_exit.set(true);
                        }
                    }
                    KeyCode::Left | KeyCode::Up => {
                        let mut next_index = match code {
                            KeyCode::Left => 0,
                            KeyCode::Up => get_next_index(active_index.get(), 1),
                            _ => unimplemented!(),
                        };

                        while options
                            .read()
                            .get(next_index)
                            .is_some_and(|opt| opt.disabled)
                        {
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

                        while options
                            .read()
                            .get(next_index)
                            .is_some_and(|opt| opt.disabled)
                        {
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
        for index in selected_index.read().iter() {
            if multiple {
                if let Some(outer_indexes) = &mut props.on_indexes {
                    outer_indexes.push(*index);
                }
            } else {
                if let Some(outer_index) = &mut props.on_index {
                    **outer_index = *index;
                }

                break;
            }
        }

        system.exit();

        return element! {
            InputFieldValue(
                label: &props.label,
                value: selected_index.read()
                    .iter()
                    .map(|index| props.options[*index].value.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
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
                        ("↕".into(), "cycle".into()),
                        ("↵".into(), "submit".into()),
                    ])
                }.into_any()
            })
        ) {
            View(flex_direction: FlexDirection::Column, margin_top: 1, margin_bottom: 1) {
                #(options.read().iter().enumerate().map(|(index, opt)| {
                    let active = active_index.get() == index;
                    let selected = selected_index.read().contains(&index);

                    element! {
                        Group(key: opt.value.clone(), gap: 1) {
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

                            Text(
                                content: &opt.label,
                                color: if !theme.supports_color {
                                    None
                                } else if opt.disabled {
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
                }))
            }
        }
    }
    .into_any()
}
