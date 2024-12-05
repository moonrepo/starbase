use super::styled_text::StyledText;
use iocraft::prelude::*;
use starbase_styles::Style;

#[derive(Default, Props)]
pub struct ContainerProps<'a> {
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Container<'a>(
    props: &mut ContainerProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let (width, _) = hooks.use_terminal_size();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            width,
        ) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct StackProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub gap: Gap,
}

#[component]
pub fn Stack<'a>(props: &mut StackProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Box(
            flex_direction: FlexDirection::Column,
            gap: props.gap,
        ) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct GroupProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub gap: Gap,
}

#[component]
pub fn Group<'a>(props: &mut GroupProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Box(
            flex_direction: FlexDirection::Row,
            gap: props.gap,
        ) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct SeparatorProps {
    pub value: String,
}

#[component]
pub fn Separator<'a>(props: &SeparatorProps) -> impl Into<AnyElement<'a>> {
    element! {
        StyledText(
            content: &props.value,
            style: Style::Muted,
            weight: Weight::Bold,
        )
    }
}
