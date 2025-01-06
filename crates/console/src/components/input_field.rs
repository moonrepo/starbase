use super::layout::Group;
use super::styled_text::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub description: Option<String>,
    pub error: Option<State<Option<String>>>,
    pub footer: Option<AnyElement<'a>>,
    pub has_focus: bool,
    pub label: String,
}

#[component]
pub fn InputField<'a>(props: &mut InputFieldProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        View(
            flex_direction: FlexDirection::Column,
            border_color: if props.has_focus {
                theme.border_focus_color
            } else {
                theme.border_color
            },
            border_edges: Edges::Left,
            border_style: BorderStyle::Round,
            padding_left: 1,
            margin_top: 1,
            margin_bottom: 1,
        ) {
            StyledText(
                content: &props.label,
                color: if theme.supports_color {
                    Some(theme.form_label_color)
                } else {
                    None
                },
                weight: Weight::Bold,
            )

            #(props.description.as_ref().map(|desc| {
                element! {
                    StyledText(content: desc)
                }
            }))

            View(width: Size::Percent(100.0)) {
                #(&mut props.children)
            }

            #(props.error.and_then(|error| {
                let error_value = error.read();

                if error_value.is_none() || error_value.as_ref().is_some_and(|v| v.is_empty()) {
                    None
                } else {
                    Some(element! {
                        StyledText(
                            content: format!(
                                "{} {}",
                                theme.form_failure_symbol,
                                error_value.as_ref().unwrap().as_str(),
                            ),
                            style: Style::Failure,
                        )
                    })
                }
            }))

            #(&mut props.footer)
        }
    }
}

#[derive(Default, Props)]
pub struct InputFieldValueProps {
    pub label: String,
    pub value: String,
}

#[component]
pub fn InputFieldValue<'a>(
    props: &InputFieldValueProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let failed = props.value.is_empty() || props.value == "false";

    element! {
        Group(gap: 1) {
            #(if failed {
                element!(StyledText(
                    content: &theme.form_failure_symbol,
                    style: Style::Failure
                ))
            } else {
                element!(StyledText(
                    content: &theme.form_success_symbol,
                    style: Style::Success
                ))
            })

            StyledText(
                content: &props.label,
                weight: Weight::Bold,
            )

            StyledText(
                content: if props.value.is_empty() {
                    &theme.layout_fallback_symbol
                } else {
                    &props.value
                },
                style: Style::MutedLight
            )
        }
    }
}

#[derive(Default, Props)]
pub struct InputLegendProps {
    pub legend: Vec<(String, String)>,
}

#[component]
pub fn InputLegend<'a>(props: &InputLegendProps) -> impl Into<AnyElement<'a>> {
    element! {
        StyledText(
            content: props.legend
                .iter()
                .map(|(key, label)| format!("<mutedlight>{key}</mutedlight> {label}"))
                .collect::<Vec<_>>()
                .join(" ⁃ "),
            style: Style::Muted
        )
    }
}
