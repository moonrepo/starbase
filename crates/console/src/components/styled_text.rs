use iocraft::prelude::*;

pub use starbase_styles::Style;

pub fn style_to_color(style: Style) -> Color {
    Color::AnsiValue(style.color() as u8)
}

#[derive(Default, Props)]
pub struct StyledTextProps {
    pub style: Option<Style>,
    pub content: String,
    pub weight: Weight,
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub decoration: TextDecoration,
}

#[component]
pub fn StyledText<'a>(props: &StyledTextProps) -> impl Into<AnyElement<'a>> {
    element! {
        Text(
            color: props.style.map(style_to_color),
            content: props.content.clone(),
            weight: props.weight,
            wrap: props.wrap,
            align: props.align,
            decoration: props.decoration
        )
    }
}
