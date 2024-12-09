use core::future::Future;

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

    // This doesn't work: iocraft types are not Send
    pub async fn render_loop_1<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
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

    // This doesn't work: the terminal hangs and there's no way to ctrl+c it
    pub fn render_loop_2<T: Component>(&self, element: Element<'_, T>) -> miette::Result<()> {
        use tokio::runtime::Handle;
        use tokio::task;

        let theme = ConsoleTheme::default();

        self.out.flush()?;

        task::block_in_place(move || {
            Handle::current().block_on(async move {
                element! {
                    ContextProvider(value: Context::owned(theme)) {
                        #(element)
                    }
                }
                .render_loop()
                .await
                .unwrap();
            });
        });

        Ok(())
    }

    // This doesn't work: iocraft types are not Send
    pub async fn render_loop_3<F: Future<Output = std::io::Result<()>>>(
        &self,
        render_future: F,
    ) -> miette::Result<()> {
        self.out.flush()?;

        render_future.await.into_diagnostic()?;

        Ok(())
    }
}
