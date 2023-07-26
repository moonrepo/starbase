#![allow(dead_code, unused_must_use)]

use miette::Diagnostic;
use starbase_events::{EventResult, EventState};
use starbase_macros::*;
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

mod starbase {
    pub use starbase_events::*;
}

#[derive(Debug, Diagnostic, Error)]
enum TestError {
    #[error("Oops")]
    Test,
}

#[derive(Debug, Event)]
#[event(dataset = i32)]
struct IntEvent(pub i32);

#[derive(Event)]
#[event(dataset = String)]
struct StringEvent(pub String);

#[derive(Event)]
#[event(dataset = PathBuf)]
struct PathEvent(pub PathBuf);

#[derive(Event)]
#[event(dataset = std::path::PathBuf)]
struct FQPathEvent(pub PathBuf);

async fn callback_func(_event: Arc<IntEvent>, data: Arc<RwLock<i32>>) -> EventResult {
    let mut data = data.write().await;
    *data += 5;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_read(data: IntEvent) -> EventResult {
    dbg!(event, data);
}

#[subscriber]
async fn callback_write(mut data: IntEvent) -> EventResult {
    *data += 5;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_write_ref(data: &mut IntEvent) -> EventResult {
    *data += 5;
}

#[subscriber]
fn callback_return(data: &mut IntEvent) {
    *data += 5;
    Ok(EventState::Stop)
}

#[subscriber]
async fn no_return(data: &mut IntEvent) -> EventResult {
    *data += 5;
}

#[subscriber]
async fn err_return(_data: IntEvent) -> EventResult {
    Err(TestError::Test.into())
}
