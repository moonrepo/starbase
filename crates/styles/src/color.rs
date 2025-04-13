// Colors based on 4th column, except for gray:
// https://upload.wikimedia.org/wikipedia/commons/1/15/Xterm_256color_chart.svg

use crate::theme::is_light_theme;
use owo_colors::{OwoColorize, XtermColors};
use std::env;
use std::path::Path;

pub use owo_colors as owo;
pub use owo_colors::Style as OwoStyle;

/// ANSI colors for a dark theme.
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

/// ANSI colors for a dark theme.
pub type DarkColor = Color;

/// ANSI colors for a light theme.
pub enum LightColor {
    White = 15,
    Black = 16,
    Teal = 29,
    Cyan = 31,
    Blue = 26,
    Green = 28,
    Purple = 99,
    Lime = 107,
    Lavender = 141,
    Red = 160,
    Brown = 94,
    Pink = 176,
    Yellow = 178,
    Orange = 202,
    Gray = 238,
    GrayLight = 241,
}

/// Types of colors based on state and usage.
#[derive(Clone, Debug, PartialEq)]
pub enum Style {
    Tag(String),

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
    /// Convert the style a specific ANSI color code, based on the current theme.
    pub fn ansi_color(&self) -> u8 {
        if is_light_theme() {
            self.light_color() as u8
        } else {
            self.dark_color() as u8
        }
    }

    /// Convert the style to a specific [Color].
    pub fn color(&self) -> Color {
        self.dark_color()
    }

    /// Convert the style to a specific [DarkColor].
    pub fn dark_color(&self) -> DarkColor {
        match self {
            Style::Caution => DarkColor::Orange,
            Style::Failure => DarkColor::Red,
            Style::Invalid => DarkColor::Yellow,
            Style::Muted => DarkColor::Gray,
            Style::MutedLight => DarkColor::GrayLight,
            Style::Success => DarkColor::Green,
            Style::File => DarkColor::Teal,
            Style::Hash => DarkColor::Green,
            Style::Id => DarkColor::Purple,
            Style::Label => DarkColor::Blue,
            Style::Path => DarkColor::Cyan,
            Style::Property => DarkColor::Lavender,
            Style::Shell => DarkColor::Pink,
            Style::Symbol => DarkColor::Lime,
            Style::Url => DarkColor::Blue,
            Style::Tag(_) => DarkColor::White,
        }
    }

    /// Convert the style to a specific [LightColor].
    pub fn light_color(&self) -> LightColor {
        match self {
            Style::Caution => LightColor::Orange,
            Style::Failure => LightColor::Red,
            Style::Invalid => LightColor::Yellow,
            Style::Muted => LightColor::Gray,
            Style::MutedLight => LightColor::GrayLight,
            Style::Success => LightColor::Green,
            Style::File => LightColor::Teal,
            Style::Hash => LightColor::Green,
            Style::Id => LightColor::Purple,
            Style::Label => LightColor::Blue,
            Style::Path => LightColor::Cyan,
            Style::Property => LightColor::Lavender,
            Style::Shell => LightColor::Pink,
            Style::Symbol => LightColor::Lime,
            Style::Url => LightColor::Blue,
            Style::Tag(_) => LightColor::Black,
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
        paint(style.ansi_color(), clean_path(value.as_ref()))
    } else {
        paint(style.ansi_color(), value)
    }
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

pub(crate) const COLOR_LIST: [u8; 76] = [
    20, 21, 26, 27, 32, 33, 38, 39, 40, 41, 42, 43, 44, 45, 56, 57, 62, 63, 68, 69, 74, 75, 76, 77,
    78, 79, 80, 81, 92, 93, 98, 99, 112, 113, 128, 129, 134, 135, 148, 149, 160, 161, 162, 163,
    164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 178, 179, 184, 185, 196, 197, 198, 199, 200,
    201, 202, 203, 204, 205, 206, 207, 208, 209, 214, 215, 220, 221,
];

pub(crate) const COLOR_LIST_UNSUPPORTED: [u8; 6] = [6, 2, 3, 4, 5, 1];
