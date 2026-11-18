//! Utilities for building, executing, and managing child processes:
//! shell-aware command construction, buffered and streamed execution
//! with optional output caching, signal handling, and sets for
//! tracking running processes and shutting them down as a group.

mod arg;
mod command;
mod env;
mod exe;
mod exec;
mod exec_capture;
mod exec_stream;
mod exec_stream_capture;
mod helpers;
mod output;
mod output_cache;
mod process_error;
mod process_set;
mod shared_child;
mod signal;
mod signal_listener;

pub use arg::*;
pub use command::*;
pub use env::*;
pub use exe::*;
pub use helpers::*;
pub use output::*;
pub use process_error::*;
pub use process_set::*;
pub use shared_child::*;
pub use signal::*;
pub use signal_listener::*;
pub use starbase_shell::{BoxedShell, ShellType};
