mod app;
mod app_state;
mod diagnostic;
mod emitters;
mod instance;
mod resources;
mod states;
mod system;

#[cfg(feature = "tracing")]
mod tracing;

pub use app::*;
pub use app_state::AppState;
pub use emitters::*;
pub use resources::*;
pub use starbase_macros::*;
pub use states::*;
pub use system::*;

pub mod diagnose {
    pub use miette::*;
    pub use thiserror::Error;
}

#[cfg(feature = "tracing")]
pub mod trace {
    pub use tracing::*;
}

pub use starbase_events as event;
pub use starbase_styles as style;
