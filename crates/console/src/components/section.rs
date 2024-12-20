use crate::ui::{ConsoleTheme, Variant};
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct SectionProps<'a> {
    pub title: String,
    pub title_color: Option<Color>,
    pub children: Vec<AnyElement<'a>>,
    pub variant: Option<Variant>,
}

#[component]
pub fn Section<'a>(props: &mut SectionProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width: Size::Percent(100.0),
        ) {
            Box(
                flex_direction: FlexDirection::Row,
                border_color: theme.border_color,
                border_edges: Edges::Top,
                border_style: BorderStyle::Round,
                width: Size::Percent(40.0)
            ) {
                Box(margin_top: -1) {
                    Text(
                        content: format!("{} ", props.title),
                        color: props
                            .title_color
                            .or_else(|| props.variant.map(|v| theme.variant(v)))
                            .unwrap_or(theme.border_focus_color),
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            #(if props.children.is_empty() {
                None
            } else {
                Some(element! {
                    Box(
                        flex_direction: FlexDirection::Column,
                        padding_top: 1,
                        padding_left: 2,
                        padding_bottom: 1,
                    ) {
                        #(&mut props.children)
                    }
                })
            })
        }
    }
}