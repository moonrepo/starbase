use crate::buffer::ConsoleBuffer;
use std::sync::Arc;

pub trait Reporter: Send + Sync {
    fn inherit_streams(&mut self, _err: Arc<ConsoleBuffer>, _out: Arc<ConsoleBuffer>) {}

    #[cfg(feature = "prompts")]
    fn inherit_theme(&mut self, _theme: Arc<crate::console::ConsoleTheme>) {}
}

pub type BoxedReporter = Box<dyn Reporter>;

#[derive(Default)]
pub struct EmptyReporter;

impl Reporter for EmptyReporter {}
