use super::layout::*;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct ListProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub gap: Option<Gap>,
}

#[component]
pub fn List<'a>(props: &mut ListProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Stack(gap: props.gap.unwrap_or(Gap::Unset)) {
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
                Separator(value: props.bullet.as_deref().unwrap_or("-"))
            }
            Box(flex_direction: FlexDirection::Column) {
                #(&mut props.children)
            }
        }
    }
}
