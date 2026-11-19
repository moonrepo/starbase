use crate::command::Command;
use crate::output::Output;
use miette::IntoDiagnostic;
use starbase_console::Reporter;
use std::future::Future;
use std::panic::resume_unwind;
use std::thread;
use tokio::runtime::{Builder, Handle, RuntimeFlavor};

/// Blocking variants of the exec methods, for sync code. Each one runs
/// the async method to completion on a tokio runtime of its own, so the
/// child is tracked in its process set, contained, and shut down on OS
/// signals like any other. They block the current thread until the child
/// exits and can't be cancelled. Called from async code, they run on a
/// thread of their own, so prefer the async methods there.
impl<R: Reporter> Command<R> {
    /// Like [`Self::exec_capture_output`], but blocks the current thread
    /// until the child exits.
    pub fn exec_capture_output_blocking(&mut self) -> miette::Result<Output> {
        block_on(self.exec_capture_output())
    }

    /// Like [`Self::exec_stream_output`], but blocks the current thread
    /// until the child exits.
    pub fn exec_stream_output_blocking(&mut self) -> miette::Result<Output> {
        block_on(self.exec_stream_output())
    }

    /// Like [`Self::exec_stream_and_capture_output`], but blocks the current
    /// thread until the child exits.
    pub fn exec_stream_and_capture_output_blocking(&mut self) -> miette::Result<Output> {
        block_on(self.exec_stream_and_capture_output())
    }
}

// Drive an exec to completion on a current-thread runtime of its own
fn block_on<F>(future: F) -> miette::Result<Output>
where
    F: Future<Output = miette::Result<Output>> + Send,
{
    let run = move || {
        Builder::new_current_thread()
            .enable_all()
            .build()
            .into_diagnostic()?
            .block_on(future)
    };

    let Ok(handle) = Handle::try_current() else {
        return run();
    };

    // A thread that's driving a runtime can't block on another one, so
    // inside a runtime context the exec runs on a thread of its own
    let scoped = || {
        thread::scope(|scope| scope.spawn(run).join()).unwrap_or_else(|panic| resume_unwind(panic))
    };

    // Let a multi-thread runtime hand this worker's other tasks to another
    // worker while it blocks
    if handle.runtime_flavor() == RuntimeFlavor::MultiThread {
        tokio::task::block_in_place(scoped)
    } else {
        scoped()
    }
}
