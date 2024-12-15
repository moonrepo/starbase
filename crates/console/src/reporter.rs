use crate::stream::ConsoleStream;
use std::fmt;

pub trait Reporter: fmt::Debug + Send + Sync {
    fn inherit_streams(&mut self, _err: ConsoleStream, _out: ConsoleStream) {}

    #[cfg(feature = "ui")]
    fn inherit_theme(&mut self, _theme: crate::theme::ConsoleTheme) {}
}

pub type BoxedReporter = Box<dyn Reporter>;

#[derive(Debug, Default)]
pub struct EmptyReporter;

impl Reporter for EmptyReporter {}
