#![allow(dead_code, unused_must_use)]

use starbase::{EventResult, EventState};
use starbase_macros::*;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

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

#[listener]
async fn callback_read(event: IntEvent) -> EventResult<IntEvent> {
    dbg!(event.0);
    Ok(EventState::Continue)
}

#[listener]
async fn callback_write(mut event: IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_write_ref(event: &mut IntEvent) -> EventResult<IntEvent> {
    event.0 += 5;
    Ok(EventState::Continue)
}

#[listener]
fn callback_return(event: &mut IntEvent) {
    event.0 += 5;
    Ok(EventState::Stop)
}
