// Colors based on 4th column, except for gray:
// https://upload.wikimedia.org/wikipedia/commons/1/15/Xterm_256color_chart.svg

use dirs::home_dir;
use owo_colors::{OwoColorize, Style as OwoStyle, XtermColors};
use std::env;
use std::path::Path;

pub enum Color {
    White = 15,
    Black = 16,
    Green = 35,
    Teal = 36,
    Cyan = 38,
    Blue = 39,
    Purple = 111,
    Lime = 112,
    Red = 161,
    Pink = 183,
    Yellow = 185,
    Gray = 239,
    GrayLight = 246,
}

pub enum Style {
    // States
    Failure,
    Invalid,
    Muted,
    MutedLight,
    Success,

    // Types
    File,
    Hash,
    Id,
    Label,
    Path,
    Shell,
    Symbol,
    Url,
}

impl Style {
    pub fn color(&self) -> Color {
        match self {
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
            Style::Shell => Color::Pink,
            Style::Symbol => Color::Lime,
            Style::Url => Color::Blue,
        }
    }
}

pub fn create_style(color: u8) -> OwoStyle {
    OwoStyle::new().color(XtermColors::from(color))
}

pub fn paint<T: AsRef<str>>(color: u8, value: T) -> String {
    if no_color() {
        value.as_ref().to_string()
    } else {
        value.as_ref().style(create_style(color)).to_string()
    }
}

pub fn paint_style<T: AsRef<str>>(style: Style, value: T) -> String {
    if matches!(style, Style::File | Style::Path | Style::Shell) {
        paint(style.color() as u8, clean_path(value.as_ref()))
    } else {
        paint(style.color() as u8, value)
    }
}

// States

pub fn failure<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Failure, value)
}

pub fn invalid<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Invalid, value)
}

pub fn muted<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Muted, value)
}

pub fn muted_light<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::MutedLight, value)
}

pub fn success<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Success, value)
}

// Types

pub fn file<T: AsRef<str>>(path: T) -> String {
    paint_style(Style::File, path)
}

pub fn hash<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Hash, value)
}

pub fn id<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Id, value)
}

pub fn label<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Label, value)
}

pub fn path<T: AsRef<Path>>(path: T) -> String {
    paint_style(Style::Path, path.as_ref().to_str().unwrap_or("<unknown>"))
}

pub fn shell<T: AsRef<str>>(cmd: T) -> String {
    paint_style(Style::Shell, cmd)
}

pub fn symbol<T: AsRef<str>>(value: T) -> String {
    paint_style(Style::Symbol, value)
}

pub fn url<T: AsRef<str>>(url: T) -> String {
    paint_style(Style::Url, url)
}

// Helpers

pub fn clean_path<T: AsRef<str>>(path: T) -> String {
    let path = path.as_ref();

    if let Some(home) = home_dir() {
        path.replace(home.to_str().unwrap_or_default(), "~")
    } else {
        path.to_string()
    }
}

// Based on https://github.com/debug-js/debug/blob/master/src/common.js#L41
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

pub fn no_color() -> bool {
    env::var("NO_COLOR").is_ok() || supports_color::on(supports_color::Stream::Stderr).is_none()
}

// 1 = 8
// 2 = 256
// 3 = 16m
pub fn supports_color() -> u8 {
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
