use crate::console::Console;
use crate::reporter::Reporter;
use iocraft::prelude::*;
use miette::IntoDiagnostic;

pub use crate::components::*;
pub use crate::theme::*;

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        let theme = ConsoleTheme::default();

        self.out.flush()?;

        element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .print();

        Ok(())
    }

    pub async fn render_loop<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        let theme = ConsoleTheme::default();

        self.out.flush()?;

        element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .render_loop()
        .await
        .into_diagnostic()?;

        Ok(())
    }
}
