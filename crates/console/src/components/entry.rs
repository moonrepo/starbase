use super::styled_text::*;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct EntryProps<'a> {
    pub name: String,
    pub content: Option<AnyElement<'a>>,
    pub separator: Option<String>,
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Entry<'a>(props: &mut EntryProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Box(flex_direction: FlexDirection::Column) {
            Box {
                Box(margin_right: 1) {
                    Text(content: &props.name)
                    StyledText(
                        content: props.separator.as_deref().unwrap_or(":"),
                        style: Style::Muted,
                        weight: Weight::Bold,
                    )
                }

                #(if props.content.is_none() {
                    if props.children.is_empty() {
                        Some(element! {
                            Box {
                                StyledText(content: "N/A", style: Style::Muted)
                            }
                        })
                    } else {
                        None
                    }
                } else {
                    Some(element! {
                        Box {
                            #(&mut props.content)
                        }
                    })
                })
            }

            #(if props.children.is_empty() {
                None
            } else {
                Some(element! {
                    Box(flex_direction: FlexDirection::Column, padding_left: 2) {
                        #(&mut props.children)
                    }
                })
            })
        }
    }
}
