use starbase_macros::*;
use std::path::PathBuf;

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
