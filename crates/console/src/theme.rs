use crate::ui::style_to_color;
use iocraft::Color;
use starbase_styles::{color::Color as NativeColor, Style};

// https://www.ditig.com/publications/256-colors-cheat-sheet
pub struct ConsoleTheme {
    pub brand_color: Color,

    // Backgrounds
    pub bg_alt_color: Color,

    // Borders
    pub border_color: Color,
    pub border_focus_color: Color,

    // Progress
    pub progress_bar_filled_char: char,
    pub progress_bar_position_char: char,
    pub progress_bar_unfilled_char: char,

    // Variants
    pub variant_caution: Color,
    pub variant_failure: Color,
    pub variant_info: Color,
    pub variant_neutral: Color,
    pub variant_success: Color,

    // Forms
    pub input_prefix_color: Color,
    pub input_prefix_active_color: Color,
    pub input_prefix_selected_color: Color,
}

impl Default for ConsoleTheme {
    fn default() -> Self {
        Self {
            brand_color: Color::White,
            bg_alt_color: Color::AnsiValue(234),
            border_color: style_to_color(Style::Muted),
            border_focus_color: style_to_color(Style::MutedLight),
            progress_bar_filled_char: '█',
            progress_bar_position_char: '▒',
            progress_bar_unfilled_char: '░',
            variant_caution: style_to_color(Style::Caution),
            variant_failure: style_to_color(Style::Failure),
            variant_info: style_to_color(Style::Label),
            variant_neutral: style_to_color(Style::Muted),
            variant_success: style_to_color(Style::Success),
            input_prefix_color: Color::AnsiValue(NativeColor::Teal as u8),
            input_prefix_active_color: Color::AnsiValue(NativeColor::Cyan as u8),
            input_prefix_selected_color: Color::AnsiValue(NativeColor::Green as u8),
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
