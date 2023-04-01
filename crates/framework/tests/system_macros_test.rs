#![allow(dead_code, unused_must_use)]

use starship::App;
use starship_macros::*;

#[derive(Debug, Event)]
struct Event1(String);

#[derive(Debug, State)]
struct State1(usize);

#[derive(Debug, State)]
struct State2(usize);

#[derive(Debug, Resource)]
struct Resource1 {
    pub field: usize,
}

#[derive(Debug, Resource)]
struct Resource2 {
    pub field: usize,
}

// READ

#[system]
async fn read_context(ctx: ContextRef) {
    dbg!(ctx);
}

#[system]
async fn read_context_renamed(other: ContextRef) {
    dbg!(other);
}

#[system]
async fn read_state_arg(arg: StateRef<State1>) {
    dbg!(arg);
}

#[system]
async fn read_state_arg_multi(arg1: StateRef<State1>, arg2: StateRef<State2>) {
    dbg!(arg1);
    dbg!(arg2);
}

#[system]
async fn read_state_same_arg(arg1: StateRef<State1>, arg2: StateRef<State1>) {
    dbg!(arg1);
    dbg!(arg2);
}

#[system]
async fn read_resource_arg(arg: ResourceRef<Resource1>) {
    dbg!(arg);
}

#[system]
async fn read_resource_arg_multi(arg1: ResourceRef<Resource1>, arg2: ResourceRef<Resource2>) {
    dbg!(arg1);
    dbg!(arg2);
}

#[system]
async fn read_resource_same_arg(arg1: ResourceRef<Resource1>, arg2: ResourceRef<Resource1>) {
    dbg!(arg1);
    dbg!(arg2);
}

// WRITE

#[system]
async fn write_context(ctx: ContextMut) {
    ctx.add_state(State1(123));
}

#[system]
async fn write_context_renamed(other: ContextMut) {
    dbg!(other);
}

#[system]
async fn write_emitter(em: EmitterMut<Event1>) {
    em.emit(Event1("test".into())).await?;
}

#[system]
async fn write_state(arg: StateMut<State1>) {
    **arg = 2;
    dbg!(arg);
}

#[system]
async fn write_resource(arg: ResourceMut<Resource1>) {
    arg.field += 2;
    dbg!(arg);
}

// MISC

#[system]
fn no_args() {
    dbg!("none");
}

#[system]
fn non_async(ctx: ContextRef) {
    dbg!(ctx);
}

// INVALID

// #[system]
// async fn fail_invalid_return() {
//     return Ok(123);
// }

// TODO?
// #[system]
// async fn fail_invalid_return_type() -> Result<usize> {
//     dbg!("fail");
// }

// #[system]
// async fn fail_self(self) {
//     dbg!(self);
// }

// #[system]
// async fn fail_unknown_type(other: ComponentRef) {
//     dbg!(other);
// }

// #[system]
// async fn fail_unknown_wrapper_type(other: OtherRef<State1>) {
//     dbg!(other);
// }

// #[system]
// async fn fail_context_with_other_args(other: ContextRef, arg: StateRef<State1>) {
//     dbg!(other);
// }

// #[system]
// async fn fail_mut_context_with_other_args(other: ContextMut, arg: StateRef<State1>) {
//     other.add_state(State1(123));
// }

// #[system]
// async fn fail_emitter_with_other_args(em: EmitterMut<Event1>, arg: StateRef<State1>) {
//     em.emit(Event1("test".into())).await?;
// }

// #[system]
// async fn fail_read_write_arg(arg1: StateRef<State1>, arg2: StateMut<State2>) {
//     dbg!(arg1);
//     **arg2 = 2;
//     dbg!(arg2);
// }

#[tokio::test]
async fn test_app() {
    let mut app = App::default();
    app.add_initializer(read_context);
    app.add_initializer(read_context_renamed);
    app.add_initializer(read_state_arg);
    app.add_initializer(read_state_arg_multi);
    app.add_initializer(read_state_same_arg);
    app.add_initializer(read_resource_arg);
    app.add_initializer(read_resource_arg_multi);
    app.add_initializer(read_resource_same_arg);
    app.add_initializer(write_context);
    app.add_initializer(write_context_renamed);
    app.add_initializer(write_emitter);
    app.add_initializer(write_state);
    app.add_initializer(write_resource);
    app.add_initializer(non_async);
    app.add_initializer(no_args);
}
