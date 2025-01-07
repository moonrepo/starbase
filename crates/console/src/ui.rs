use crate::console::Console;
use crate::reporter::Reporter;
use iocraft::prelude::*;
use miette::IntoDiagnostic;
use std::env;

pub use crate::components::*;
pub use crate::theme::*;

fn is_forced_tty() -> bool {
    env::var("STARBASE_FORCE_TTY").is_ok()
}

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        let is_tty = is_forced_tty() || self.out.is_terminal();

        let mut theme = self.theme();
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

        let buffer = self.out.buffer();

        if is_tty {
            canvas.write_ansi(buffer).into_diagnostic()?;
        } else {
            canvas.write(buffer).into_diagnostic()?;
        }

        self.out.flush()?;

        Ok(())
    }

    pub async fn render_interactive<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> miette::Result<()> {
        let is_tty = is_forced_tty() || self.out.is_terminal();

        // If not a TTY, exit immediately
        if !is_tty {
            return Ok(());
        }

        self.render_loop(element).await
    }

    pub async fn render_loop<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        let is_tty = is_forced_tty() || self.out.is_terminal();

        let mut theme = self.theme();
        theme.supports_color = env::var("NO_COLOR").is_err() && is_tty;

        self.out.flush()?;

        element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .render_loop()
        .await
        .into_diagnostic()?;

        self.out.flush()?;

        Ok(())
    }
}
