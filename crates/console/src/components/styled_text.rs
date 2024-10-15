use iocraft::prelude::*;
use starbase_styles::Style;

#[derive(Props)]
pub struct StyledTextProps {
    pub style: Style,
    pub content: String,
    pub weight: Weight,
    pub wrap: TextWrap,
    pub align: TextAlign,
    pub decoration: TextDecoration,
}

#[component]
pub fn StyledText(props: &StyledTextProps) -> impl Into<AnyElement<'static>> {
    element! {
        Text(
            color: Color::AnsiValue(props.style as u8),
            content: props.content.clone(),
            weight: props.weight,
            wrap: props.wrap,
            align: props.align,
            decoration: props.decoration
        )
    }
}
