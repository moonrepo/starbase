use crate::color::paint_style;
use std::path::PathBuf;

pub use crate::color::Style;

pub trait Stylize {
    fn style(&self, style: Style) -> String;
}

impl Stylize for String {
    fn style(&self, style: Style) -> String {
        paint_style(style, self)
    }
}

impl Stylize for PathBuf {
    fn style(&self, style: Style) -> String {
        paint_style(style, self.to_string_lossy())
    }
}
