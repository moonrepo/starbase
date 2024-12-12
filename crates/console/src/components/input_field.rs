use super::styled_text::*;
use crate::ui::ConsoleTheme;
use iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub description: Option<&'a str>,
    pub error: Option<State<String>>,
    pub footer: Option<AnyElement<'a>>,
    pub label: &'a str,
    pub label_color: Option<Color>,
}

#[component]
pub fn InputField<'a>(props: &mut InputFieldProps<'a>, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();

    element! {
        Box(
            flex_direction: FlexDirection::Column,
            border_color: theme.border_color,
            border_edges: Edges::Left,
            border_style: BorderStyle::Bold,
            padding_left: 1,
        ) {
            Text(
                content: props.label,
                color: props.label_color.unwrap_or(theme.brand_color),
                weight: Weight::Bold,
            )

            #(props.description.map(|desc| {
                element! {
                    StyledText(content: desc)
                }
            }))

            Box(margin_top: 1) {
                #(&mut props.children)
            }

            #(if props.error.is_some() || props.footer.is_some() {
                Some(
                    element! {
                        Box(margin_top: 1, flex_direction: FlexDirection::Column) {
                            #(props.error.map(|error| {
                                element! {
                                    StyledText(
                                        content: error.read().as_str(),
                                        style: Style::Failure,
                                    )
                                }
                            }))

                            #(&mut props.footer)
                        }
                    }
                )
            } else {
                None
            })
        }
    }
}
