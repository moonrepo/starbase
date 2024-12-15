use crate::console::Console;
use crate::reporter::Reporter;
use iocraft::prelude::*;
use miette::IntoDiagnostic;

pub use crate::components::*;
pub use crate::theme::*;

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        let theme = self.theme();
        let is_tty = self.out.is_terminal();

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
        let theme = self.theme();

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
