use std::sync::Arc;
use std::sync::atomic::{AtomicI16, Ordering};

/// A thread-safe exit code that can be shared across sessions.
#[derive(Clone, Debug)]
pub struct AppExitCode(Arc<AtomicI16>);

impl AppExitCode {
    /// Get the current exit code, if any. If no exit code has been set, this will return `None`.
    pub fn get(&self) -> Option<u8> {
        let code = self.0.load(Ordering::Relaxed);

        if code == -1 { None } else { Some(code as u8) }
    }

    /// Set the exit code to a specific value. This will override any previous exit code set.
    pub fn set(&self, code: u8) {
        self.0.store(code as i16, Ordering::Relaxed);
    }
}

impl Default for AppExitCode {
    fn default() -> Self {
        Self(Arc::new(AtomicI16::new(-1)))
    }
}
