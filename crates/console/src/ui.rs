use crate::console::Console;
use crate::reporter::Reporter;
use iocraft::prelude::*;
use miette::IntoDiagnostic;

pub use crate::components::*;

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        self.out.flush()?;
        element.into_any().print();

        Ok(())
    }

    pub async fn render_loop<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        self.out.flush()?;
        element.into_any().render_loop().await.into_diagnostic()?;

        Ok(())
    }
}
