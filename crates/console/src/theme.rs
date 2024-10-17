use crate::ui::style_to_color;
use iocraft::Color;
use starbase_styles::Style;

pub struct ConsoleTheme {
    pub border_color: Color,
    pub border_focus_color: Color,
}

impl Default for ConsoleTheme {
    fn default() -> Self {
        Self {
            border_color: style_to_color(Style::Muted),
            border_focus_color: style_to_color(Style::MutedLight),
        }
    }
}
