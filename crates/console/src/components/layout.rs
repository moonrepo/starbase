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
    let (mut width, _) = hooks.use_terminal_size();

    // Non-TTY's like CI environments
    if width == 0 {
        width = 60;
    }

    dbg!("width", width);

    element! {
        View(
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
        View(
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
        View(
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
