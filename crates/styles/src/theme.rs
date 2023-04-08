use crate::color::{self, Color};
use miette::{GraphicalTheme, ThemeStyles};

pub fn create_graphical_theme() -> GraphicalTheme {
    let mut theme = GraphicalTheme::unicode();

    if let Some(supports) = supports_color::on(supports_color::Stream::Stderr) {
        if supports.has_256 || supports.has_16m {
            theme.styles = ThemeStyles {
                error: color::style(Color::Red as u8),
                warning: color::style(Color::Yellow as u8),
                advice: color::style(Color::Teal as u8),
                help: color::style(Color::Purple as u8),
                link: color::style(Color::Blue as u8),
                linum: color::style(Color::GrayLight as u8),
                highlights: vec![
                    color::style(Color::Green as u8),
                    color::style(Color::Teal as u8),
                    color::style(Color::Blue as u8),
                    color::style(Color::Purple as u8),
                    color::style(Color::Pink as u8),
                    color::style(Color::Red as u8),
                ],
            };
        } else {
            theme.styles = ThemeStyles::ansi();
        }
    } else {
        theme.styles = ThemeStyles::none();
    }

    theme
}
