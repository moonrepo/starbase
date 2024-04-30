use crate::app::Phase;
use starbase_macros::{system, State};

// This is a hack for starbase macros to work from within
// the starbase crate itself!
mod starbase {
    pub use crate::*;
}

#[derive(Debug, State)]
pub struct AppPhase {
    pub phase: Phase,
}

#[system]
pub async fn start_startup_phase(states: States) {
    states.set(AppPhase {
        phase: Phase::Startup,
    });
}

#[system]
pub async fn start_analyze_phase(app_state: StateMut<AppPhase>) {
    app_state.phase = Phase::Analyze;
}

#[system]
pub async fn start_execute_phase(app_state: StateMut<AppPhase>) {
    app_state.phase = Phase::Execute;
}

#[system]
pub async fn start_shutdown_phase(app_state: StateMut<AppPhase>) {
    app_state.phase = Phase::Shutdown;
}
