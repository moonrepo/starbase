use super::input_field::*;
use super::layout::Group;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;
use std::collections::HashSet;

#[derive(Clone, Default)]
pub struct SelectOption {
    pub description: Option<String>,
    pub disabled: bool,
    pub label: Option<String>,
    pub value: String,
}

impl SelectOption {
    pub fn new(value: impl AsRef<str>) -> Self {
        let value = value.as_ref();

        Self {
            description: None,
            disabled: false,
            label: None,
            value: value.to_owned(),
        }
    }

    pub fn description(self, description: impl AsRef<str>) -> Self {
        Self {
            description: Some(description.as_ref().to_owned()),
            ..self
        }
    }

    pub fn description_opt(self, description: Option<String>) -> Self {
        Self {
            description,
            ..self
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
            label: Some(label.as_ref().to_owned()),
            ..self
        }
    }

    pub fn label_opt(self, label: Option<String>) -> Self {
        Self { label, ..self }
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
    pub separator: String,
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
            separator: "- ".into(),
            on_index: None,
            on_indexes: None,
        }
    }
}

#[component]
pub fn Select<'a>(
    props: &mut SelectProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let (_, height) = hooks.use_terminal_size();
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
    let last_index = options.read().len() - 1;
    let (start_index, end_index) = calculate_indexes(
        active_index.get(),
        last_index,
        ((height / 2).max(17) - 2) as usize,
    );

    // let (out, _) = hooks.use_output();
    // out.println(format!(
    //     "active = {}, max = {}, start = {start_index}, end = {end_index}, limit = {}",
    //     active_index.get(),
    //     last_index,
    //     ((height / 2).max(17) - 2)
    // ));

    let get_next_index = move |current: usize, step: isize| -> usize {
        let next = current as isize - step;

        if next < 0 {
            last_index
        } else if next > last_index as isize {
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
                            KeyCode::Right => last_index,
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
                #((start_index > 0).then(|| {
                    element! {
                        View(padding_left: 2) {
                            Text(
                                content: format!("...{start_index} more"),
                                color: theme.style_muted_light_color
                            )
                        }
                    }
                }))

                #(options.read()[start_index..=end_index].iter().enumerate().map(|(iter_index, opt)| {
                    let index = start_index + iter_index;
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
                                content: opt.label.as_ref().unwrap_or(&opt.value),
                                color: if !theme.supports_color {
                                    None
                                } else if opt.disabled {
                                    Some(theme.style_muted_color)
                                } else if selected {
                                    Some(theme.input_selected_color)
                                } else if active {
                                    Some(theme.input_active_color)
                                } else {
                                    None
                                }
                            )

                            #(opt.description.as_ref().map(|desc| {
                                element! {
                                    Text(
                                        content: format!("{}{desc}", props.separator),
                                        color: theme.style_muted_light_color
                                    )
                                }
                            }))
                        }
                    }
                }))

                #((end_index < last_index).then(|| {
                    element! {
                        View(padding_left: 2) {
                            Text(
                                content: format!("...{} more", last_index - end_index),
                                color: theme.style_muted_light_color
                            )
                        }
                    }
                }))
            }
        }
    }
    .into_any()
}

fn calculate_indexes(active_index: usize, max_index: usize, limit: usize) -> (usize, usize) {
    if max_index <= limit {
        return (0, max_index);
    }

    let before_limit = limit / 2;
    let after_limit = limit / 2 - (if limit % 2 == 0 { 1 } else { 0 });
    let start_index;
    let end_index;

    if active_index <= before_limit {
        start_index = 0;
        end_index = limit - 1;
    } else if active_index > max_index - after_limit {
        start_index = max_index - limit + 1;
        end_index = max_index;
    } else {
        start_index = active_index - before_limit;
        end_index = active_index + after_limit;
    }

    (start_index, end_index)
}
