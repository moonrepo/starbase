use super::styled_text::*;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct EntryGroupProps<'a> {
    pub title: String,
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn EntryGroup<'a>(props: &mut EntryGroupProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Box(
            flex_direction: FlexDirection::Column,
            margin_top: 1,
            // margin_bottom: 1,
            width: Size::Percent(100.0),
        ) {
            Box(
                flex_direction: FlexDirection::Row,
                border_color: style_to_color(Style::Muted),
                border_edges: Edges::Top,
                border_style: BorderStyle::Single,
                width: Size::Percent(40.0)
            ) {
                Box(margin_top: -1) {
                    StyledText(
                        content: format!("{} ", props.title),
                        style: Style::MutedLight,
                        weight: Weight::Bold,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            Box(
                flex_direction: FlexDirection::Column,
                padding_left: 2
            ) {
                #(&mut props.children)
            }
        }
    }
}

#[derive(Default, Props)]
pub struct EntryProps<'a> {
    pub title: String,
    pub content: String,
    pub style: Option<Style>,
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Entry<'a>(props: &mut EntryProps<'a>) -> impl Into<AnyElement<'a>> {
    assert!(
        !(!props.content.is_empty() && !props.children.is_empty()),
        "Cannot use content and children props together"
    );

    let prefix = element! {
        Box(margin_right: 1) {
            Text(content: format!("{}:", props.title))
        }
    };

    // Stacked when children
    if !props.children.is_empty() {
        return element! {
            Box(flex_direction: FlexDirection::Column) {
                #(prefix)

                Box(padding_left: 2) {
                    #(&mut props.children)
                }
            }
        };
    }

    // Grouped when just the content
    element! {
        Box {
            #(prefix)

            Box {
                StyledText(
                    content: props.content.clone(),
                    style: props.style.unwrap_or(Style::MutedLight),
                )
            }
        }
    }
}
