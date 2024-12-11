use super::styled_text::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub description: Option<String>,
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
            border_style: BorderStyle::Bold,
            padding_left: 1,
        ) {
            Text(
                content: &props.label,
                color: props.label_color.unwrap_or(theme.brand_color),
                weight: Weight::Bold,
            )

            #(props.description.as_ref().map(|desc| {
                element! {
                    StyledText(content: desc)
                }
            }))

            #(&mut props.children)
        }
    }
}
