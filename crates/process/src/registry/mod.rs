//! A registry that owns the lifetime of child processes: tracking them
//! while they run, reaping them when they exit, forwarding termination
//! signals to them (and their descendants), shutting them down as a group,
//! and caching their output. See [`ProcessRegistry`].

mod cache;
mod child;
mod event;
mod options;
mod process_registry;
mod state;
mod tasks;
mod tree;

pub use child::*;
pub use event::*;
pub use options::*;
pub use process_registry::*;
