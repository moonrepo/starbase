use crate::ui::ConsoleTheme;
use iocraft::prelude::*;
use starbase_styles::parse_tags;

pub use starbase_styles::Style;

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
pub fn StyledText<'a>(
    props: &StyledTextProps,
    hooks: Hooks,
) -> impl Into<AnyElement<'a>> + use<'a> {
    let theme = hooks.use_context::<ConsoleTheme>();

    let contents = parse_tags(&props.content, false)
        .into_iter()
        .map(|(text, tag)| {
            let mut content = MixedTextContent::new(text);

            if theme.supports_color {
                if let Some(color) = tag
                    .as_ref()
                    .and_then(|tag| theme.tag_to_color(tag))
                    .or_else(|| {
                        props
                            .style
                            .as_ref()
                            .and_then(|style| theme.style_to_color(style))
                    })
                    .or(props.color)
                {
                    content = content.color(color);
                }
            }

            if props.weight != Weight::Normal {
                content = content.weight(props.weight);
            }

            if props.decoration != TextDecoration::None {
                content = content.decoration(props.decoration);
            }

            content
        })
        .collect::<Vec<_>>();

    element! {
        MixedText(contents, wrap: props.wrap, align: props.align)
    }
}
