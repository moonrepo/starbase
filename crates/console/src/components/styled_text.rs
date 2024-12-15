use iocraft::prelude::*;
use starbase_styles::color::parse_style_tags;

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
pub fn StyledText<'a>(props: &StyledTextProps) -> impl Into<AnyElement<'a>> {
    let parts = parse_style_tags(&props.content);

    element! {
        Box {
            #(parts.into_iter().map(|(text, style)| {
                element! {
                    Text(
                        color: style.or(props.style).map(style_to_color).or(props.color),
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
