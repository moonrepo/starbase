use crate::ui::ConsoleTheme;
use iocraft::prelude::*;
use starbase_styles::color::parse_tags;

pub use starbase_styles::Style;

pub fn style_to_color(style: Style) -> Color {
    Color::AnsiValue(style.color() as u8)
}

#[derive(Default, Props)]
pub struct StyledTextProps {
    pub color: Option<Color>,
    pub style: Option<Style>,
    pub content: String,
    pub weight: Weight,
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub decoration: TextDecoration,
}

#[component]
pub fn StyledText<'a>(props: &StyledTextProps, hooks: Hooks) -> impl Into<AnyElement<'a>> + use<'a> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let parts = parse_tags(&props.content);

    element! {
        View {
            #(parts.into_iter().map(|(text, tag)| {
                element! {
                    Text(
                        color: if theme.supports_color {
                            tag.as_ref()
                                .and_then(|tag| theme.tag_to_color(tag))
                                .or_else(|| props.style.as_ref().and_then(|style| theme.style_to_color(style)))
                                .or(props.color)
                        } else {
                            None
                        },
                        content: text,
                        weight: props.weight,
                        wrap: props.wrap,
                        align: props.align,
                        decoration: props.decoration
                    )
                }
            }))
        }
    }
}
