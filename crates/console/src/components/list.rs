use super::layout::*;
use super::styled_text::*;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct ListProps<'a> {
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn List<'a>(props: &mut ListProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Stack(gap: 0) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct ListItemProps<'a> {
    pub bullet: Option<String>,
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn ListItem<'a>(props: &mut ListItemProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Box(
            flex_direction: FlexDirection::Row,
        ) {
            Box(padding_right: 1) {
                StyledText(
                    content: props.bullet.as_deref().unwrap_or("▪"),
                    style: Style::Muted,
                )
            }
            Box {
                #(&mut props.children)
            }
        }
    }
}
