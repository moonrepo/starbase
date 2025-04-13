use std::sync::OnceLock;

/// Return true if the current theme is "light" based on the
/// `STARBASE_THEME` environment variable.
pub fn is_light_theme() -> bool {
    static LIGHT_THEME: OnceLock<bool> = OnceLock::new();

    *LIGHT_THEME.get_or_init(|| std::env::var("STARBASE_THEME").is_ok_and(|value| value == "light"))
}

/// Create a graphical theme for use in `miette`.
#[cfg(feature = "theme")]
pub fn create_graphical_theme() -> miette::GraphicalTheme {
    use crate::color::{self, DarkColor, LightColor};
    use miette::{GraphicalTheme, ThemeStyles};

    let mut theme = GraphicalTheme::unicode();

    if let Some(supports) = supports_color::on(supports_color::Stream::Stderr) {
        if supports.has_256 || supports.has_16m {
            let is_light = is_light_theme();

            let code = |light: LightColor, dark: DarkColor| -> u8 {
                if is_light { light as u8 } else { dark as u8 }
            };

            theme.styles = ThemeStyles {
                error: color::create_style(code(LightColor::Red, DarkColor::Red)),
                warning: color::create_style(code(LightColor::Yellow, DarkColor::Yellow)),
                advice: color::create_style(code(LightColor::Teal, DarkColor::Teal)),
                help: color::create_style(code(LightColor::Purple, DarkColor::Purple)),
                link: color::create_style(code(LightColor::Blue, DarkColor::Blue)),
                linum: color::create_style(code(LightColor::GrayLight, DarkColor::GrayLight)),
                highlights: vec![
                    color::create_style(code(LightColor::Green, DarkColor::Green)),
                    color::create_style(code(LightColor::Teal, DarkColor::Teal)),
                    color::create_style(code(LightColor::Blue, DarkColor::Blue)),
                    color::create_style(code(LightColor::Purple, DarkColor::Purple)),
                    color::create_style(code(LightColor::Pink, DarkColor::Pink)),
                    color::create_style(code(LightColor::Red, DarkColor::Red)),
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
