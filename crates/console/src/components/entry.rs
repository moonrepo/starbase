use super::layout::*;
use super::styled_text::*;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct EntryProps<'a> {
    pub name: String,
    pub value: Option<AnyElement<'a>>,
    pub content: Option<String>,
    pub fallback: Option<String>,
    pub separator: Option<String>,
    pub children: Vec<AnyElement<'a>>,
    pub no_children: bool,
}

#[component]
pub fn Entry<'a>(props: &mut EntryProps<'a>) -> impl Into<AnyElement<'a>> {
    let no_children = props.no_children || props.children.is_empty();

    element! {
        Box(flex_direction: FlexDirection::Column) {
            Box {
                Box(margin_right: 1) {
                    Text(content: &props.name)
                    Separator(value: props.separator.as_deref().unwrap_or(":"))
                }

                #(if props.value.is_some() {
                    Some(element! {
                        Box {
                            #(&mut props.value)
                        }
                    })
                } else if let Some(content) = &props.content {
                    Some(element! {
                        Box {
                            StyledText(content, style: Style::MutedLight)
                        }
                    })
                } else if no_children {
                    Some(element! {
                        Box {
                            StyledText(
                                content: props.fallback.as_deref().unwrap_or("—"),
                                style: Style::Muted,
                            )
                        }
                    })
                } else {
                    None
                })
            }

            #(if no_children {
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
