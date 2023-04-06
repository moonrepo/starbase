#![allow(dead_code, unused_must_use)]

use miette::Diagnostic;
use starbase_events::{EventResult, EventState};
use starbase_macros::*;
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Diagnostic, Error)]
enum TestError {
    #[error("Oops")]
    Test,
}

#[derive(Event)]
#[event(value = "i32")]
struct IntEvent(pub i32);

#[derive(Event)]
#[event(value = "String")]
struct StringEvent(pub String);

#[derive(Event)]
#[event(value = "PathBuf")]
struct PathEvent(pub PathBuf);

#[derive(Event)]
#[event(value = "std::path::PathBuf")]
struct FQPathEvent(pub PathBuf);

async fn callback_func(event: Arc<RwLock<IntEvent>>) -> EventResult<IntEvent> {
    let mut event = event.write().await;
    event.0 += 5;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_read(event: IntEvent) -> EventResult<IntEvent> {
    dbg!(event.0);
}

#[subscriber]
async fn callback_write(mut event: IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_write_ref(event: &mut IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
}

#[subscriber]
fn callback_return(event: &mut IntEvent) {
    event.0 += 5;
    Ok(EventState::Stop)
}

#[subscriber]
async fn no_return(event: &mut IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
}

#[subscriber]
async fn custom_return(event: &mut IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
    Ok(EventState::Return(123))
}

#[subscriber]
async fn err_return(_event: IntEvent) -> EventResult<IntEvent> {
    Err(TestError::Test.into())
}
