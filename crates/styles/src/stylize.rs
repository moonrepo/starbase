use crate::color::paint_style;
use std::path::PathBuf;

pub use crate::color::Style;

pub trait Stylize {
    /// Wrap the current value in the given style (an ANSI color escape code).
    fn style(&self, style: Style) -> String;
}

impl Stylize for &'static str {
    fn style(&self, style: Style) -> String {
        paint_style(style, self)
    }
}

impl Stylize for String {
    fn style(&self, style: Style) -> String {
        paint_style(style, self)
    }
}

impl Stylize for PathBuf {
    fn style(&self, style: Style) -> String {
        paint_style(style, self.to_str().unwrap_or("<unknown>"))
    }
}

macro_rules! extend_integer {
    ($type:ident) => {
        impl Stylize for $type {
            fn style(&self, style: Style) -> String {
                paint_style(style, self.to_string())
            }
        }
    };
}

extend_integer!(u8);
extend_integer!(u16);
extend_integer!(u32);
extend_integer!(u64);
extend_integer!(u128);
extend_integer!(usize);
extend_integer!(i8);
extend_integer!(i16);
extend_integer!(i32);
extend_integer!(i64);
extend_integer!(i128);
extend_integer!(isize);
