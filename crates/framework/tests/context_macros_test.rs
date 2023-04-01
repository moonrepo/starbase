#![allow(dead_code, unused_must_use)]

use starship::RelativePathBuf;
use starship_macros::*;
use std::path::PathBuf;

// STATE

#[derive(Debug, State)]
struct State1;

#[derive(Debug, State)]
struct State2(usize);

#[derive(Debug, State)]
struct State3 {
    count: usize,
}

#[derive(Debug, State)]
enum State4 {
    One,
    Two,
    Three,
}

#[derive(Debug, State)]
struct StatePath(PathBuf);

#[derive(Debug, State)]
struct StateRelPath(RelativePathBuf);

// RESOURCE

#[derive(Debug, Resource)]
struct Resource1;

#[derive(Debug, Resource)]
struct Resource2(usize);

#[derive(Debug, Resource)]
struct Resource3 {
    count: usize,
}

#[derive(Debug, Resource)]
enum Resource4 {
    One,
    Two,
    Three,
}
