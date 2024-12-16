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
pub fn StyledText<'a>(props: &StyledTextProps, hooks: Hooks) -> impl Into<AnyElement<'a>> {
    let theme = hooks.use_context::<ConsoleTheme>();
    let parts = parse_tags(&props.content);

    element! {
        Box {
            #(parts.into_iter().map(|(text, tag)| {
                element! {
                    Text(
                        color: tag.and_then(tag_to_style)
                            .or(props.style)
                            .map(|style| theme.style(style))
                            .or(props.color),
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

fn tag_to_style(tag: String) -> Option<Style> {
    let style = match tag.as_str() {
        "caution" => Style::Caution,
        "failure" => Style::Failure,
        "invalid" => Style::Invalid,
        "muted" => Style::Muted,
        "mutedlight" | "muted_light" => Style::MutedLight,
        "success" => Style::Success,
        "file" => Style::File,
        "hash" | "version" => Style::Hash,
        "id" => Style::Id,
        "label" => Style::Label,
        "path" => Style::Path,
        "property" => Style::Property,
        "shell" => Style::Shell,
        "url" => Style::Url,
        _ => return None,
    };

    Some(style)
}
