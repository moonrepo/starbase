#![allow(dead_code, unused_must_use)]

use starbase::{App, Emitter};
use starbase_macros::*;

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

#[derive(Clone, Debug)]
struct SomeArgs {}

// READ

#[system]
async fn read_states(states: States) {
    dbg!(states);
}

#[system]
async fn read_states_renamed(other: States) {
    dbg!(other);
}

#[system]
async fn read_state_arg(arg: StateRef<State1>) {
    dbg!(arg);
}

#[system(instrument = false)]
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
async fn read_sub_state(args: StateRef<ExecuteArgs, SomeArgs>) {
    dbg!(args);
}

#[system]
async fn read_args_ref(args: Args<SomeArgs>) {
    dbg!(args);
}

#[system]
async fn read_resources(resources: Resources) {
    dbg!(resources);
}

#[system]
async fn read_resources_renamed(other: Resources) {
    dbg!(other);
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

#[system]
async fn read_emitters(emitters: Emitters) {
    emitters.get::<Emitter<Event1>>();
}

#[system]
async fn read_emitter(em: EmitterRef<Event1>) {
    em.emit(Event1("test".into())).await?;
}

#[system]
async fn read_all_managers(states: States, resources: Resources, emitters: Emitters) {
    dbg!(states, resources, emitters);
}

// WRITE

#[system]
async fn write_states(states: States) {
    states.set(State1(123));
}

#[system]
async fn write_states_renamed(other: States) {
    dbg!(other);
}

#[system]
async fn write_state(arg: StateMut<State1>) {
    **arg = 2;
    dbg!(arg);
}

#[system]
async fn write_resources(resources: Resources) {
    resources.set(Resource1 { field: 123 });
}

#[system(instrument = false)]
async fn write_resources_renamed(other: Resources) {
    dbg!(other);
}

#[system]
async fn write_resource(arg: ResourceMut<Resource1>) {
    arg.field += 2;
    dbg!(arg);
}

#[system]
async fn write_emitters(emitters: Emitters) {
    emitters.set(Emitter::<Event1>::new());
}

#[system]
async fn write_emitters_renamed(other: Emitters) {
    dbg!(other);
}

#[system]
async fn write_emitter(em: EmitterMut<Event1>) {
    em.emit(Event1("test".into())).await?;
}

#[system]
async fn write_all_managers(states: States, resources: Resources, emitters: Emitters) {
    states.set(State1(123));
    resources.set(Resource1 { field: 123 });
    emitters.set(Emitter::<Event1>::new());
}

// MISC

#[system]
fn default_params_renamed(a: States, b: Resources, c: Emitters) {
    dbg!(a, b, c);
}

#[system(instrument = false)]
fn no_args() {
    dbg!("none");
}

#[system]
fn non_async() {
    dbg!("none");
}

#[system]
async fn manager_with_other_args(manager: States, _arg: StateRef<State1>) {
    dbg!(&manager);
}

#[system]
async fn mut_manager_with_other_args(manager: States, _arg: StateRef<State1>) {
    manager.set(State1(123));
}

#[system]
async fn raw(_raw1: StateRaw<State1>, _raw2: StateRaw<State2>) {}

// INVALID

// #[system]
// async fn fail_invalid_return() {
//     return Ok(123);
// }

// // TODO?
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
// async fn fail_read_write_arg(arg1: StateRef<State1>, arg2: StateMut<State2>) {
//     dbg!(arg1);
//     **arg2 = 2;
//     dbg!(arg2);
// }

// #[system]
// async fn fail_multi_write_arg(arg1: StateMut<State1>, arg2: StateMut<State2>) {
//     **arg1 = 2;
//     **arg2 = 2;
// }

#[tokio::test]
async fn test_app() {
    let mut app = App::new();
    app.startup(read_states);
    app.startup(read_states_renamed);
    app.startup(read_state_arg);
    app.startup(read_state_arg_multi);
    app.startup(read_state_same_arg);
    app.startup(read_resources);
    app.startup(read_resources_renamed);
    app.startup(read_resource_arg);
    app.startup(read_resource_arg_multi);
    app.startup(read_resource_same_arg);
    app.startup(read_all_managers);
    app.startup(write_states);
    app.startup(write_states_renamed);
    app.startup(write_state);
    app.startup(write_resources);
    app.startup(write_resources_renamed);
    app.startup(write_resource);
    app.startup(write_emitters);
    app.startup(write_emitters_renamed);
    app.startup(write_emitter);
    app.startup(write_all_managers);
    app.startup(non_async);
    app.startup(no_args);
}
