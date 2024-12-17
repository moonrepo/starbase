use super::layout::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct MapProps<'a> {
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn Map<'a>(props: &mut MapProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Stack(gap: 0) {
            #(&mut props.children)
        }
    }
}

#[derive(Default, Props)]
pub struct MapItemProps<'a> {
    pub separator: Option<String>,
    pub name: Option<AnyElement<'a>>,
    pub value: Option<AnyElement<'a>>,
}

#[component]
pub fn MapItem<'a>(props: &mut MapItemProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Box(
            flex_direction: FlexDirection::Row,
        ) {
            Box {
                #(&mut props.name)
            }
            Box(padding_left: 1, padding_right: 1) {
                Separator(value: props.separator.as_deref().unwrap_or(&theme.layout_map_separator))
            }
            Box(flex_direction: FlexDirection::Column) {
                #(&mut props.value)
            }
        }
    }
}
