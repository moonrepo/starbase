use crate::reporter::*;
use crate::stream::*;
use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tracing::trace;

#[cfg(feature = "prompts")]
pub type ConsoleTheme = inquire::ui::RenderConfig<'static>;

pub struct Console<R: Reporter> {
    pub err: ConsoleStream,
    err_handle: Option<JoinHandle<()>>,

    pub out: ConsoleStream,
    out_handle: Option<JoinHandle<()>>,

    quiet: Arc<AtomicBool>,
    reporter: Option<Arc<Box<R>>>,

    #[cfg(feature = "prompts")]
    theme: Arc<ConsoleTheme>,
}

impl<R: Reporter> Console<R> {
    pub fn new(quiet: bool) -> Self {
        trace!("Creating buffered console");

        let quiet = Arc::new(AtomicBool::new(quiet));

        let mut err = ConsoleStream::new(ConsoleStreamType::Stderr);
        err.quiet = Some(Arc::clone(&quiet));

        let mut out = ConsoleStream::new(ConsoleStreamType::Stdout);
        out.quiet = Some(Arc::clone(&quiet));

        Self {
            err_handle: err.handle.take(),
            err,
            out_handle: out.handle.take(),
            out,
            quiet,
            reporter: None,
            #[cfg(feature = "prompts")]
            theme: Arc::new(crate::prompts::create_theme()),
        }
    }

    pub fn new_testing() -> Self {
        Self {
            err: ConsoleStream::new_testing(ConsoleStreamType::Stderr),
            err_handle: None,
            out: ConsoleStream::new_testing(ConsoleStreamType::Stdout),
            out_handle: None,
            quiet: Arc::new(AtomicBool::new(false)),
            reporter: None,
            #[cfg(feature = "prompts")]
            theme: Arc::new(ConsoleTheme::empty()),
        }
    }

    pub fn close(&mut self) -> miette::Result<()> {
        trace!("Closing console and flushing buffered output");

        self.err.close()?;
        self.out.close()?;

        if let Some(handle) = self.err_handle.take() {
            let _ = handle.join();
        }

        if let Some(handle) = self.out_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    pub fn quiet(&self) {
        self.quiet.store(true, Ordering::Release);
    }

    pub fn stderr(&self) -> ConsoleStream {
        self.err.clone()
    }

    pub fn stdout(&self) -> ConsoleStream {
        self.out.clone()
    }

    pub fn reporter(&self) -> Arc<Box<R>> {
        Arc::clone(
            self.reporter
                .as_ref()
                .expect("Reporter has not been configured for the current console!"),
        )
    }

    #[cfg(feature = "prompts")]
    pub fn theme(&self) -> Arc<ConsoleTheme> {
        Arc::clone(&self.theme)
    }

    pub fn set_reporter(&mut self, mut reporter: R) {
        reporter.inherit_streams(self.stderr(), self.stdout());

        #[cfg(feature = "prompts")]
        reporter.inherit_theme(self.theme());

        self.reporter = Some(Arc::new(Box::new(reporter)));
    }

    #[cfg(feature = "prompts")]
    pub fn set_theme(&mut self, theme: ConsoleTheme) {
        self.theme = Arc::new(theme);
    }
}

impl<R: Reporter> Clone for Console<R> {
    fn clone(&self) -> Self {
        Self {
            err: self.err.clone(),
            err_handle: None,
            out: self.out.clone(),
            out_handle: None,
            quiet: self.quiet.clone(),
            reporter: self.reporter.clone(),
            #[cfg(feature = "prompts")]
            theme: self.theme.clone(),
        }
    }
}

impl<R: Reporter> fmt::Debug for Console<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Console")
            .field("err", &self.err)
            .field("out", &self.out)
            .field("quiet", &self.quiet)
            .field("reporter", &self.reporter)
            .finish()
    }
}

impl<R: Reporter> Deref for Console<R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        self.reporter
            .as_ref()
            .expect("Reporter has not been configured for the current console!")
    }
}
