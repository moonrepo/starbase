use super::styled_text::StyledText;
use iocraft::prelude::*;
use starbase_styles::Style;
use std::env;

/// Terminal width for one-shot renders, where iocraft has no terminal
/// to report a size from.
pub(crate) struct RenderWidth(pub u16);

pub(crate) fn use_render_width(hooks: &mut Hooks) -> u16 {
    let (width, _) = hooks.use_terminal_size();

    if width > 0 {
        width
    } else {
        hooks
            .try_use_context::<RenderWidth>()
            .map_or(0, |render_width| render_width.0)
    }
}

#[derive(Default, Props)]
pub struct ContainerProps<'a> {
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Container<'a>(
    props: &mut ContainerProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let mut width = use_render_width(&mut hooks);

    // Non-TTY's like CI environments
    if width == 0 || env::var("STARBASE_TEST").is_ok() {
        width = env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(60);
    }

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
pub fn Stack<'a>(props: &mut StackProps<'a>) -> impl Into<AnyElement<'a>> + use<'a> {
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
pub fn Group<'a>(props: &mut GroupProps<'a>) -> impl Into<AnyElement<'a>> + use<'a> {
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
pub fn Separator<'a>(props: &SeparatorProps) -> impl Into<AnyElement<'a>> + use<'a> {
    element! {
        StyledText(
            content: &props.value,
            style: Style::Muted,
            weight: Weight::Bold,
        )
    }
}
