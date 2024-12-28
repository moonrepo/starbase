use super::layout::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct MapProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub gap: Gap,
}

#[component]
pub fn Map<'a>(props: &mut MapProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Stack(gap: props.gap) {
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
        Group(gap: 1) {
            Box {
                #(&mut props.name)
            }

            Separator(value: props.separator.as_deref().unwrap_or(&theme.layout_map_separator))

            Stack {
                #(&mut props.value)
            }
        }
    }
}
