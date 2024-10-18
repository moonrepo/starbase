// Colors based on 4th column, except for gray:
// https://upload.wikimedia.org/wikipedia/commons/1/15/Xterm_256color_chart.svg

use owo_colors::{OwoColorize, XtermColors};
use std::env;
use std::path::Path;

pub use owo_colors as owo;
pub use owo_colors::Style as OwoStyle;

pub enum Color {
    White = 15,
    Black = 16,
    Teal = 36,
    Cyan = 38,
    Blue = 39,
    Green = 41,
    Purple = 111,
    Lime = 112,
    Lavender = 147,
    Red = 161,
    Brown = 172,
    Pink = 183,
    Yellow = 185,
    Orange = 208,
    Gray = 239,
    GrayLight = 246,
}

#[derive(Copy, Clone, Debug)]
pub enum Style {
    // States
    Caution,
    Failure,
    Invalid,
    Muted,
    MutedLight,
    Success,

    // Types
    File,     // rel file paths, file names/exts
    Hash,     // hashes, shas, commits
    Id,       // ids, names
    Label,    // titles, strings
    Path,     // abs file paths
    Property, // properties, keys, fields, settings
    Shell,    // shell, cli, commands
    Symbol,   // symbols, chars
    Url,      // urls
}

impl Style {
    /// Convert the style to a specific [Color].
    pub fn color(&self) -> Color {
        match self {
            Style::Caution => Color::Orange,
            Style::Failure => Color::Red,
            Style::Invalid => Color::Yellow,
            Style::Muted => Color::Gray,
            Style::MutedLight => Color::GrayLight,
            Style::Success => Color::Green,
            Style::File => Color::Teal,
            Style::Hash => Color::Green,
            Style::Id => Color::Purple,
            Style::Label => Color::Blue,
            Style::Path => Color::Cyan,
            Style::Property => Color::Lavender,
            Style::Shell => Color::Pink,
            Style::Symbol => Color::Lime,
            Style::Url => Color::Blue,
        }
    }
}

/// Create a new `owo_colors` [Style][OwoStyle] instance and apply the given color.
pub fn create_style(color: u8) -> OwoStyle {
    OwoStyle::new().color(XtermColors::from(color))
}

/// Paint and wrap the string with the appropriate ANSI color escape code.
/// If colors are disabled, the string is returned as-is.
pub fn paint<T: AsRef<str>>(color: u8, value: T) -> String {
    if no_color() {
        value.as_ref().to_string()
    } else {
        value.as_ref().style(create_style(color)).to_string()
    }
}

/// Paint the string with the given style.
pub fn paint_style<T: AsRef<str>>(style: Style, value: T) -> String {
    if matches!(style, Style::File | Style::Path | Style::Shell) {
        paint(style.color() as u8, clean_path(value.as_ref()))
    } else {
        paint(style.color() as u8, value)
    }
}

pub fn parse_style_tags<T: AsRef<str>>(value: T) -> Vec<(String, Style)> {
    let mut message = value.as_ref().to_owned();
    let mut result = vec![];

    let mut last_index = 0;
    let mut curr_index = 0;

    result
}

/// Apply styles to a string by replacing style specific tags.
/// For example, `<file>starbase.json</file>`.
pub fn apply_style_tags<T: AsRef<str>>(value: T) -> String {
    let mut message = value.as_ref().to_owned();

    if !message.contains('<') {
        return message;
    }

    for style in [
        Style::Caution,
        Style::Failure,
        Style::File,
        Style::Hash,
        Style::Id,
        Style::Invalid,
        Style::Label,
        Style::Muted,
        Style::MutedLight,
        Style::Path,
        Style::Property,
        Style::Shell,
        Style::Success,
        Style::Symbol,
        Style::Url,
    ] {
        let tag_name = format!("{:?}", style).to_lowercase();
        let open_tag = format!("<{}>", tag_name);
        let close_tag = format!("</{}>", tag_name);

        while let Some(open_index) = message.find(&open_tag) {
            if let Some(close_index) = message.find(&close_tag) {
                let inner = &message[open_index + open_tag.len()..close_index];

                message = message.replace(
                    &format!("{}{}{}", open_tag, inner, close_tag),
                    &paint_style(style, inner),
                );
            } else {
                // No closing? Just remove the opening...
                message = message.replace(&open_tag, "");
            }
        }
    }

    message
}

// States

/// Paint a caution state.
pub fn caution<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Caution, value)
}

/// Paint a failure state.
pub fn failure<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Failure, value)
}

/// Paint an invalid state.
pub fn invalid<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Invalid, value)
}

/// Paint a muted dark state.
pub fn muted<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Muted, value)
}

/// Paint a muted light state.
pub fn muted_light<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::MutedLight, value)
}

/// Paint a success state.
pub fn success<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Success, value)
}

// Types

/// Paint a partial file path or glob pattern.
pub fn file<T: AsRef<str>>(path: T) -> String {
    paint_style(Style::File, path)
}

/// Paint a hash-like value.
pub fn hash<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Hash, value)
}

/// Paint an identifier.
pub fn id<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Id, value)
}

/// Paint a label, heading, or title.
pub fn label<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Label, value)
}

/// Paint an absolute file path.
pub fn path<T: AsRef<Path>>(path: T) -> String {
    paint_style(Style::Path, path.as_ref().to_str().unwrap_or("<unknown>"))
}

/// Paint an relative file path.
#[cfg(feature = "relative-path")]
pub fn rel_path<T: AsRef<relative_path::RelativePath>>(path: T) -> String {
    paint_style(Style::Path, path.as_ref().as_str())
}

/// Paint a property, key, or setting.
pub fn property<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Property, value)
}

/// Paint a shell command or input string.
pub fn shell<T: AsRef<str>>(cmd: T) -> String {
    paint_style(Style::Shell, cmd)
}

/// Paint a symbol, value, or number.
pub fn symbol<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Symbol, value)
}

/// Paint a URL.
pub fn url<T: AsRef<str>>(url: T) -> String {
    paint_style(Style::Url, url)
}

// Helpers

/// Clean a file system path by replacing the home directory with `~`.
pub fn clean_path<T: AsRef<str>>(path: T) -> String {
    let path = path.as_ref();

    #[cfg(not(target_arch = "wasm32"))]
    if let Some(home) = dirs::home_dir() {
        return path.replace(home.to_str().unwrap_or_default(), "~");
    }

    path.to_string()
}

/// Dynamically apply a color to the log target/module/namespace based
/// on the characters in the string.
pub fn log_target<T: AsRef<str>>(value: T) -> String {
    let value = value.as_ref();
    let mut hash: u32 = 0;

    for b in value.bytes() {
        hash = (hash << 5).wrapping_sub(hash) + b as u32;
    }

    // Lot of casting going on here...
    if supports_color() >= 2 {
        let index = i32::abs(hash as i32) as usize % COLOR_LIST.len();

        return paint(COLOR_LIST[index], value);
    }

    let index = i32::abs(hash as i32) as usize % COLOR_LIST_UNSUPPORTED.len();

    paint(COLOR_LIST_UNSUPPORTED[index], value)
}

/// Return true if color has been disabled for the `stderr` stream.
#[cfg(not(target_arch = "wasm32"))]
pub fn no_color() -> bool {
    env::var("NO_COLOR").is_ok() || supports_color::on(supports_color::Stream::Stderr).is_none()
}

#[cfg(target_arch = "wasm32")]
pub fn no_color() -> bool {
    true
}

/// Return a color level support for the `stderr` stream. 0 = no support, 1 = basic support,
/// 2 = 256 colors, and 3 = 16 million colors.
pub fn supports_color() -> u8 {
    if no_color() {
        return 0;
    }

    if let Some(support) = supports_color::on(supports_color::Stream::Stderr) {
        if support.has_16m {
            return 3;
        } else if support.has_256 {
            return 2;
        } else if support.has_basic {
            return 1;
        }
    }

    1
}

pub const COLOR_LIST: [u8; 76] = [
    20, 21, 26, 27, 32, 33, 38, 39, 40, 41, 42, 43, 44, 45, 56, 57, 62, 63, 68, 69, 74, 75, 76, 77,
    78, 79, 80, 81, 92, 93, 98, 99, 112, 113, 128, 129, 134, 135, 148, 149, 160, 161, 162, 163,
    164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 178, 179, 184, 185, 196, 197, 198, 199, 200,
    201, 202, 203, 204, 205, 206, 207, 208, 209, 214, 215, 220, 221,
];

pub const COLOR_LIST_UNSUPPORTED: [u8; 6] = [6, 2, 3, 4, 5, 1];
