use crate::reporter::*;
use crate::stream::*;
#[cfg(feature = "ui")]
use crate::theme::ConsoleTheme;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use tracing::trace;

pub struct Console<R: Reporter> {
    pub err: ConsoleStream,
    err_handle: Option<JoinHandle<()>>,

    pub out: ConsoleStream,
    out_handle: Option<JoinHandle<()>>,

    quiet: Arc<AtomicBool>,
    reporter: Option<Arc<R>>,

    #[cfg(feature = "ui")]
    theme: ConsoleTheme,
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
            #[cfg(feature = "ui")]
            theme: Default::default(),
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
            #[cfg(feature = "ui")]
            theme: Default::default(),
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

    pub fn reporter(&self) -> Arc<R> {
        Arc::clone(
            self.reporter
                .as_ref()
                .expect("Reporter has not been configured for the current console!"),
        )
    }

    #[cfg(feature = "ui")]
    pub fn theme(&self) -> ConsoleTheme {
        self.theme.clone()
    }

    pub fn set_reporter(&mut self, mut reporter: R) {
        reporter.inherit_streams(self.stderr(), self.stdout());

        #[cfg(feature = "ui")]
        reporter.inherit_theme(self.theme());

        self.reporter = Some(Arc::new(reporter));
    }

    #[cfg(feature = "ui")]
    pub fn set_theme(&mut self, theme: crate::theme::ConsoleTheme) {
        if let Some(arc_reporter) = &mut self.reporter {
            if let Some(reporter) = Arc::get_mut(arc_reporter) {
                reporter.inherit_theme(theme.clone());
            }
        }

        self.theme = theme;
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
            #[cfg(feature = "ui")]
            theme: self.theme.clone(),
        }
    }
}

impl<R: Reporter> fmt::Debug for Console<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("Console");

        dbg.field("err", &self.err)
            .field("out", &self.out)
            .field("quiet", &self.quiet)
            .field("reporter", &self.reporter);

        #[cfg(feature = "ui")]
        dbg.field("theme", &self.theme);

        dbg.finish()
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
