use crate::ui::style_to_color;
use iocraft::Color;
use starbase_styles::{Style, color::Color as NativeColor};
use std::collections::HashMap;

// https://www.ditig.com/publications/256-colors-cheat-sheet
#[derive(Clone, Debug)]
pub struct ConsoleTheme {
    pub brand_color: Color,

    // Backgrounds
    pub bg_alt_color: Color,

    // Borders
    pub border_color: Color,
    pub border_focus_color: Color,

    // Forms
    pub form_label_color: Color,
    pub form_failure_symbol: String,
    pub form_success_symbol: String,

    // Inputs
    pub input_active_color: Color,
    pub input_prefix_color: Color,
    pub input_prefix_symbol: String,
    pub input_selected_color: Color,
    pub input_selected_symbol: String,

    // Layout
    pub layout_fallback_symbol: String,
    pub layout_list_bullet: String,
    pub layout_map_separator: String,

    // Progress
    pub progress_bar_color: Color,
    pub progress_bar_filled_char: char,
    pub progress_bar_position_char: char,
    pub progress_bar_unfilled_char: char,
    pub progress_loader_color: Color,
    pub progress_loader_frames: Vec<String>,

    // Styles (variants)
    pub style_caution_color: Color,
    pub style_failure_color: Color,
    pub style_info_color: Color,
    pub style_invalid_color: Color,
    pub style_neutral_color: Color,
    pub style_muted_color: Color,
    pub style_muted_light_color: Color,
    pub style_success_color: Color,

    // Styles (types)
    pub style_file_color: Color,
    pub style_hash_color: Color,
    pub style_id_color: Color,
    pub style_label_color: Color,
    pub style_path_color: Color,
    pub style_property_color: Color,
    pub style_shell_color: Color,
    pub style_symbol_color: Color,
    pub style_url_color: Color,

    // Misc
    pub supports_color: bool,
    pub custom_tags: HashMap<String, Color>,
}

impl Default for ConsoleTheme {
    fn default() -> Self {
        Self {
            brand_color: Color::White,
            bg_alt_color: Color::AnsiValue(234),
            border_color: style_to_color(Style::Muted),
            border_focus_color: style_to_color(Style::MutedLight),
            form_label_color: Color::White,
            form_failure_symbol: "✘".into(),
            form_success_symbol: "✔".into(),
            input_active_color: Color::AnsiValue(NativeColor::Cyan as u8),
            input_prefix_color: Color::White,
            input_prefix_symbol: "❯".into(),
            input_selected_color: Color::AnsiValue(NativeColor::Teal as u8),
            input_selected_symbol: "✔".into(),
            layout_fallback_symbol: "—".into(),
            layout_list_bullet: "-".into(),
            layout_map_separator: "=".into(),
            progress_bar_color: Color::White,
            progress_bar_filled_char: '█',
            progress_bar_position_char: '▒',
            progress_bar_unfilled_char: '░',
            progress_loader_color: Color::White,
            progress_loader_frames: DEFAULT_FRAMES.iter().map(|f| f.to_string()).collect(),
            style_caution_color: style_to_color(Style::Caution),
            style_failure_color: style_to_color(Style::Failure),
            style_info_color: style_to_color(Style::Label),
            style_invalid_color: style_to_color(Style::Invalid),
            style_neutral_color: style_to_color(Style::Muted),
            style_muted_color: style_to_color(Style::Muted),
            style_muted_light_color: style_to_color(Style::MutedLight),
            style_success_color: style_to_color(Style::Success),
            style_file_color: style_to_color(Style::File),
            style_hash_color: style_to_color(Style::Hash),
            style_id_color: style_to_color(Style::Id),
            style_label_color: style_to_color(Style::Label),
            style_path_color: style_to_color(Style::Path),
            style_property_color: style_to_color(Style::Property),
            style_shell_color: style_to_color(Style::Shell),
            style_symbol_color: style_to_color(Style::Symbol),
            style_url_color: style_to_color(Style::Url),
            supports_color: true,
            custom_tags: HashMap::new(),
        }
    }
}

impl ConsoleTheme {
    pub fn branded(color: Color) -> Self {
        Self {
            brand_color: color,
            input_prefix_color: color,
            progress_bar_color: color,
            progress_loader_color: color,
            ..Self::default()
        }
    }

    pub fn style_to_color(&self, style: &Style) -> Option<Color> {
        let color = match style {
            Style::Caution => self.style_caution_color,
            Style::Failure => self.style_failure_color,
            Style::Invalid => self.style_invalid_color,
            Style::Muted => self.style_muted_color,
            Style::MutedLight => self.style_muted_light_color,
            Style::Success => self.style_success_color,
            Style::File => self.style_file_color,
            Style::Hash => self.style_hash_color,
            Style::Id => self.style_id_color,
            Style::Label => self.style_label_color,
            Style::Path => self.style_path_color,
            Style::Property => self.style_property_color,
            Style::Shell => self.style_shell_color,
            Style::Symbol => self.style_symbol_color,
            Style::Url => self.style_url_color,
            Style::Tag(tag) => return self.custom_tags.get(tag).cloned(),
        };

        Some(color)
    }

    pub fn tag_to_color(&self, tag: &str) -> Option<Color> {
        self.style_to_color(&match tag {
            "caution" => Style::Caution,
            "failure" => Style::Failure,
            "invalid" => Style::Invalid,
            "muted" => Style::Muted,
            "mutedlight" => Style::MutedLight,
            "success" => Style::Success,
            "file" => Style::File,
            "hash" => Style::Hash,
            "id" => Style::Id,
            "label" => Style::Label,
            "path" => Style::Path,
            "property" => Style::Property,
            "shell" => Style::Shell,
            "symbol" => Style::Symbol,
            "url" => Style::Url,
            tag => Style::Tag(tag.to_owned()),
        })
    }

    pub fn variant(&self, variant: Variant) -> Color {
        match variant {
            Variant::Caution => self.style_caution_color,
            Variant::Failure => self.style_failure_color,
            Variant::Info => self.style_info_color,
            Variant::Neutral => self.style_neutral_color,
            Variant::Success => self.style_success_color,
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

const DEFAULT_FRAMES: &[&str] = &["▏", "▎", "▍", "▌", "▋", "▊", "▉", "▊", "▋", "▌", "▍", "▎"];
