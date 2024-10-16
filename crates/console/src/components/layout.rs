use iocraft::prelude::*;

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
            padding_bottom: 1,
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
