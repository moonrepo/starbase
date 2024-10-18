use crate::ui::style_to_color;
use iocraft::Color;
use starbase_styles::Style;

pub struct ConsoleTheme {
    // Borders
    pub border_color: Color,
    pub border_focus_color: Color,

    // Variants
    pub variant_caution: Color,
    pub variant_failure: Color,
    pub variant_info: Color,
    pub variant_neutral: Color,
    pub variant_success: Color,
}

impl Default for ConsoleTheme {
    fn default() -> Self {
        Self {
            border_color: style_to_color(Style::Muted),
            border_focus_color: style_to_color(Style::MutedLight),
            variant_caution: style_to_color(Style::Caution),
            variant_failure: style_to_color(Style::Failure),
            variant_info: style_to_color(Style::Label),
            variant_neutral: style_to_color(Style::Muted),
            variant_success: style_to_color(Style::Success),
        }
    }
}

impl ConsoleTheme {
    pub fn variant(&self, variant: Variant) -> Color {
        match variant {
            Variant::Caution => self.variant_caution,
            Variant::Failure => self.variant_failure,
            Variant::Info => self.variant_info,
            Variant::Neutral => self.variant_neutral,
            Variant::Success => self.variant_success,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum Variant {
    Caution,
    Failure,
    Info,
    #[default]
    Neutral,
    Success,
}
