use super::layout::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct ListProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub gap: Gap,
}

#[component]
pub fn List<'a>(props: &mut ListProps<'a>) -> impl Into<AnyElement<'a>> {
    element! {
        Stack(gap: props.gap) {
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
pub fn ListItem<'a>(props: &mut ListItemProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Group(gap: 1) {
            Separator(value: props.bullet.as_deref().unwrap_or(&theme.layout_list_bullet))

            Stack {
                #(&mut props.children)
            }
        }
    }
}
