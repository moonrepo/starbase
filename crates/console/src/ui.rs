use crate::console::Console;
use crate::reporter::Reporter;
use miette::IntoDiagnostic;

pub use iocraft;
pub use iocraft::prelude::*;

#[component]
fn Counter(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut count = hooks.use_state(|| 0);

    hooks.use_future(async move {
        count += 1;
    });

    element! {
        Text(color: Color::Blue, content: format!("counter: {}", count))
    }
}

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
