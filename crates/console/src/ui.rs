use crate::console::Console;
use crate::console_error::ConsoleError;
use crate::reporter::Reporter;
use crate::stream::{ConsoleStream, ConsoleStreamType};
use iocraft::prelude::*;
use std::env;

pub use crate::components::*;
pub use crate::theme::*;

fn is_forced_tty() -> bool {
    env::var("STARBASE_FORCE_TTY").is_ok_and(|value| !value.is_empty())
}

fn is_ignoring_ctrl_c() -> bool {
    env::var("STARBASE_IGNORE_CTRL_C").is_ok_and(|value| !value.is_empty())
}

pub struct RenderOptions {
    pub handle_interrupt: bool,
    pub fullscreen: bool,
    pub ignore_ctrl_c: bool,
    pub stream: ConsoleStreamType,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            handle_interrupt: false,
            fullscreen: false,
            ignore_ctrl_c: is_ignoring_ctrl_c(),
            stream: ConsoleStreamType::Stdout,
        }
    }
}

impl RenderOptions {
    pub fn stderr() -> Self {
        Self {
            stream: ConsoleStreamType::Stderr,
            ..Default::default()
        }
    }

    pub fn stdout() -> Self {
        Self::default()
    }
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
        options: RenderOptions,
    ) -> Result<(), ConsoleError> {
        let is_tty = is_forced_tty() || self.is_terminal();

        // If not a TTY, exit immediately
        if !is_tty {
            return Ok(());
        }

        self.render_loop(element, theme, options).await
    }

    pub async fn render_loop<T: Component>(
        &self,
        element: Element<'_, T>,
        mut theme: ConsoleTheme,
        options: RenderOptions,
    ) -> Result<(), ConsoleError> {
        let is_tty = is_forced_tty() || self.is_terminal();

        theme.supports_color = env::var("NO_COLOR").is_err() && is_tty;

        self.flush()?;

        let mut element = element! {
            ContextProvider(value: Context::owned(theme)) {
                #(element)
            }
        }
        .into_any();

        if options.handle_interrupt {
            element = element! {
                SignalContainer {
                    #(element)
                }
            }
            .into_any();
        }

        let mut renderer = element.render_loop();

        if options.fullscreen {
            renderer = renderer.fullscreen();
        }

        if options.handle_interrupt || options.ignore_ctrl_c {
            renderer = renderer.ignore_ctrl_c();
        }

        renderer.await.map_err(|error| ConsoleError::RenderFailed {
            error: Box::new(error),
        })?;

        self.flush()?;

        if options.handle_interrupt && received_interrupt_signal() {
            std::process::exit(130);
        }

        Ok(())
    }
}

impl<R: Reporter> Console<R> {
    pub fn render<T: Component>(&self, element: Element<'_, T>) -> Result<(), ConsoleError> {
        self.render_with_options(element, RenderOptions::stdout())
    }

    pub fn render_err<T: Component>(&self, element: Element<'_, T>) -> Result<(), ConsoleError> {
        self.render_with_options(element, RenderOptions::stderr())
    }

    pub fn render_with_options<T: Component>(
        &self,
        element: Element<'_, T>,
        options: RenderOptions,
    ) -> Result<(), ConsoleError> {
        match options.stream {
            ConsoleStreamType::Stderr => self.err.render(element, self.theme()),
            ConsoleStreamType::Stdout => self.out.render(element, self.theme()),
        }
    }

    pub async fn render_interactive<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.render_interactive_with_options(element, RenderOptions::stdout())
            .await
    }

    pub async fn render_interactive_err<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.render_interactive_with_options(element, RenderOptions::stderr())
            .await
    }

    pub async fn render_interactive_with_options<T: Component>(
        &self,
        element: Element<'_, T>,
        options: RenderOptions,
    ) -> Result<(), ConsoleError> {
        match options.stream {
            ConsoleStreamType::Stderr => {
                self.err
                    .render_interactive(element, self.theme(), options)
                    .await
            }
            ConsoleStreamType::Stdout => {
                self.out
                    .render_interactive(element, self.theme(), options)
                    .await
            }
        }
    }

    pub async fn render_loop<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.render_loop_with_options(element, RenderOptions::stdout())
            .await
    }

    pub async fn render_loop_err<T: Component>(
        &self,
        element: Element<'_, T>,
    ) -> Result<(), ConsoleError> {
        self.render_loop_with_options(element, RenderOptions::stderr())
            .await
    }

    pub async fn render_loop_with_options<T: Component>(
        &self,
        element: Element<'_, T>,
        options: RenderOptions,
    ) -> Result<(), ConsoleError> {
        match options.stream {
            ConsoleStreamType::Stderr => self.err.render_loop(element, self.theme(), options).await,
            ConsoleStreamType::Stdout => self.out.render_loop(element, self.theme(), options).await,
        }
    }
}
