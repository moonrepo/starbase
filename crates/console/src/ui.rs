use crate::console::Console;
use crate::console_error::ConsoleError;
use crate::reporter::Reporter;
use crate::stream::ConsoleStream;
use iocraft::prelude::*;
use std::env;

pub use crate::components::*;
pub use crate::theme::*;

fn is_forced_tty() -> bool {
    env::var("STARBASE_FORCE_TTY").is_ok()
}

impl ConsoleStream {
    pub fn render<T: Component>(
        &self,
        element: Element<'_, T>,
        mut theme: ConsoleTheme,
    ) -> Result<(), ConsoleError> {
        let is_tty = is_forced_tty() || self.is_terminal();

        theme.supports_color = env::var("NO_COLOR").is_err() && is_tty;

        let canvas = element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .render(if is_tty {
            crossterm::terminal::size().ok().map(|size| size.0 as usize)
        } else {
            None
        });

        let buffer = self.buffer();

        if is_tty {
            canvas
                .write_ansi(buffer)
                .map_err(|error| ConsoleError::RenderFailed {
                    error: Box::new(error),
                })?;
        } else {
            canvas
                .write(buffer)
                .map_err(|error| ConsoleError::RenderFailed {
                    error: Box::new(error),
                })?;
        }

        self.flush()?;

        Ok(())
    }

    pub async fn render_interactive<T: Component>(
        &self,
        element: Element<'_, T>,
        theme: ConsoleTheme,
    ) -> Result<(), ConsoleError> {
        let is_tty = is_forced_tty() || self.is_terminal();

        // If not a TTY, exit immediately
        if !is_tty {
            return Ok(());
        }

        self.render_loop(element, theme).await
    }

    pub async fn render_loop<T: Component>(
        &self,
        element: Element<'_, T>,
        mut theme: ConsoleTheme,
    ) -> Result<(), ConsoleError> {
        let is_tty = is_forced_tty() || self.is_terminal();

        theme.supports_color = env::var("NO_COLOR").is_err() && is_tty;

        self.flush()?;

        element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .render_loop()
        .await
        .map_err(|error| ConsoleError::RenderFailed {
            error: Box::new(error),
        })?;

        self.flush()?;

        Ok(())
    }
}

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> Result<(), ConsoleError> {
        self.out.render(element, self.theme())
    }

    pub async fn render_interactive<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.out.render_interactive(element, self.theme()).await
    }

    pub async fn render_loop<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.out.render_loop(element, self.theme()).await
    }
}
