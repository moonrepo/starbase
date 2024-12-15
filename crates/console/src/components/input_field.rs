use super::styled_text::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub description: Option<String>,
    pub error: Option<State<Option<String>>>,
    pub footer: Option<AnyElement<'a>>,
    pub label: String,
    pub label_color: Option<Color>,
}

#[component]
pub fn InputField<'a>(props: &mut InputFieldProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_color: theme.border_color,
            border_edges: Edges::Left,
            border_style: BorderStyle::Single,
            padding_left: 1,
        ) {
            StyledText(
                content: &props.label,
                color: props.label_color.unwrap_or(theme.form_label_color),
                weight: Weight::Bold,
            )

            #(props.description.as_ref().map(|desc| {
                element! {
                    StyledText(content: desc)
                }
            }))

            Box(width: Size::Percent(100.0)) {
                #(&mut props.children)
            }

            #(props.error.and_then(|error| {
                let error_value = error.read();

                if error_value.is_none() || error_value.as_ref().is_some_and(|v| v.is_empty()) {
                    None
                } else {
                    Some(element! {
                        StyledText(
                            content: format!("✘ {}", error_value.as_ref().unwrap().as_str()),
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
    pub label_color: Option<Color>,
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
        Box {
            #(if failed {
                element!(Text(
                    content: "✘",
                    color: theme.variant_failure
                ))
            } else {
                element!(Text(
                    content: "✔",
                    color: theme.variant_success
                ))
            })

            Box(width: 1)

            StyledText(
                content: &props.label,
                color: props.label_color.unwrap_or(theme.form_label_color),
                weight: Weight::Bold,
            )

            Box(width: 1)

            StyledText(
                content: &props.value,
                style: Style::MutedLight
            )
        }
    }
}
