use crate::console::Console;
use crate::reporter::Reporter;
use miette::IntoDiagnostic;

pub use crate::components::*;
pub use iocraft;
pub use iocraft::prelude::*;

impl<R: Reporter> Console<R> {
    pub fn render(&self, mut element: AnyElement) -> miette::Result<()> {
        self.out.flush()?;
        element.print();

        Ok(())
    }

    pub async fn render_loop(&self, mut element: AnyElement<'_>) -> miette::Result<()> {
        self.out.flush()?;
        element.render_loop().await.into_diagnostic()?;

        Ok(())
    }
}
