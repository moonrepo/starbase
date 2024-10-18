use super::styled_text::*;
use crate::ui::{ConsoleTheme, Variant};
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct NoticeProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub no_title: bool,
    pub title: Option<String>,
    pub variant: Option<Variant>,
}

#[component]
pub fn Notice<'a>(props: &mut NoticeProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    let title = if props.no_title {
        None
    } else if props.title.is_some() {
        props.title.clone()
    } else {
        match props.variant.unwrap_or_default() {
            Variant::Caution => Some("Caution".into()),
            Variant::Failure => Some("Failure".into()),
            Variant::Success => Some("Success".into()),
            _ => None,
        }
    };

    let color = props
        .variant
        .map(|v| theme.variant(v))
        .unwrap_or(theme.border_color);

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_color: color,
            border_edges: Edges::Left,
            border_style: BorderStyle::Bold,
            margin: 1,
            padding_left: 1,
        ) {
            #(title.map(|title| {
                element! {
                    Text(content: title.to_uppercase(), color, weight: Weight::Bold)
                }
            }))

            #(&mut props.children)
        }
    }
}
