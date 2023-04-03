use crate::app::Phase;
use starship_macros::{system, State};

// This is a hack for starship macros to work from within
// the starship crate itself!
mod starship {
    pub use crate::*;
}

#[derive(Debug, State)]
pub struct AppState {
    pub current_phase: Phase,
}

#[system]
pub async fn start_initialize_phase(states: StatesMut) {
    states.set(AppState {
        current_phase: Phase::Initialize,
    });
}

#[system]
pub async fn start_analyze_phase(app_state: StateMut<AppState>) {
    app_state.current_phase = Phase::Analyze;
}

#[system]
pub async fn start_execute_phase(app_state: StateMut<AppState>) {
    app_state.current_phase = Phase::Analyze;
}

#[system]
pub async fn start_finalize_phase(app_state: StateMut<AppState>) {
    app_state.current_phase = Phase::Finalize;
}
