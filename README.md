# starship

Starship is a framework for building performant command line applications or processing pipelines.
It takes heavy inspiration from the popular
[ECS pattern](https://en.wikipedia.org/wiki/Entity_component_system) but works quite differently.

A starship is built with the following modules:

- **Reactor core** - Async-first powered by the `tokio` runtime.
- **Warp drive** - Thread-safe concurrent systems for easy processing.
- **Communication array** - Event-driven architecture to decouple and isolate crates.
- **Shield generator** - Native diagnostics and reports with `miette`.
- **Navigation sensors** -

### Roadmap

- [x] Async application layer built on `tokio`
  - [x] Systems
  - [x] States/resources
  - [x] Event emitters
- [ ] Logging + tracing via the `tracing` crate
  - [ ] Include `metrics`?
- [x] Error handling + diagnostics via the `miette` crate
  - [x] Replace `anyhow`

### Current issues

Some things I have yet to resolve... help appreciated!

1. I'm not entirely sure if the event emitter implementation is the best choice to go with. We can't
   use channels because we need to mutate the event in listeners, and we also need to support async.

2. Async functions cannot be registered as listeners to `on()` or `once()` because of "mismatched
   but equal" lifetime issues. This is the following error:

```
error[E0308]: mismatched types
  --> crates/framework/tests/events_test.rs:179:5
    |
179 |     emitter.on(callback_func);
    |     ^^^^^^^^^^^^^^^^^^^^^^^^^ one type is more general than the other
    |
    = note: expected trait `for<'a> <for<'a> fn(&'a mut TestEvent) -> impl Future<Output = Result<EventState<<TestEvent as starship::Event>::Value>, ErrReport>> {callback_func} as FnOnce<(&'a mut TestEvent,)>>`
               found trait `for<'a> <for<'a> fn(&'a mut TestEvent) -> impl Future<Output = Result<EventState<<TestEvent as starship::Event>::Value>, ErrReport>> {callback_func} as FnOnce<(&'a mut TestEvent,)>>`
```

# Core

## Phases

An application is divided into phases, where [systems](#systems) in each phase will be processed and
completed before moving onto the next phase. The following phases are available:

- **Startup** - Register and or load [components](#components) into the application instance.
  - Example: load configuration, detect workspace root, load plugins
- **Analyze** - Analyze the current application environment, update components, and prepare for
  execution.
  - Example: generate project graph, load cache, signin to service
- **Execute** - Execute primary business logic.
  - Example: process dependency graph, run generator, check for new version
- **Shutdown** - Shutdown whether a success or failure.
  - Example: cleanup temporary files

The startup phase processes systems serially in the main thread, as the order of initializations
must be deterministic, and running in parallel may cause race conditions or unwanted side-effects.

The other 3 phases process systems concurrently by spawning a new thread for each system. Active
systems are constrained using a semaphore and available CPU count. If a system fails, the
application will abort and subsequent systems will not run (excluding shutdown systems).

## Systems

Systems are async functions that implement the `System` trait, are added to an application phase,
and are processed (only once) during the applications run cycle. Systems receive each
[component type](#components) as a distinct parameter.

> Systems are loosely based on the S in ECS that Bevy and other game engines utilize. The major
> difference is that our systems are async only, run once, and do not require the entity (E) or
> component (C) parts.

```rust
use starship::{States, Resources, Emitters, MainResult, SystemResult};

async fn load_config(states: States, resources: Resources, emitters: Emitters) -> SystemResult {
  let states = states.write().await;

  let config: AppConfig = do_load_config();
  states.set::<AppConfig>(config);

  Ok(())
}

#[tokio::main]
async fn main() -> MainResult {
  let mut app = App::new();
  app.startup(load_config);
  app.run().await?;

  Ok(())
}
```

Each system parameter type (`States`, `Resources`, `Emitters`) is a type alias that wraps the
underlying component manager in a `Arc<RwLock<T>>`, allowing for distinct read/write locks per
component type. Separating components across params simplifies borrow semantics.

Furthermore, for better ergonomics and developer experience, we provide a `#[system]` function
attribute that provides "magic" parameters similar to Axum and Bevy, which we call system
parameters. For example, the above system can be rewritten as:

```rust
#[system]
async fn load_config(states: StatesMut) {
  let config: AppConfig = do_load_config();
  states.set::<AppConfig>(config);
}
```

Which compiles down to the following, while taking mutable and immutable borrowship rules into
account. If a rule is broken, we panic during compilation.

```rust
async fn load_config(
  states: starship::States,
  resources: starship::Resources,
  emitters: starship::Emitters,
) -> starship::SystemResult {
    let mut states = states.write().await;
    {
        let config: AppConfig = do_load_config();
        states.set::<AppConfig>(config);
    }
    Ok(())
}
```

Additional benefits of `#[system]` are:

- Return type and return statement are both optional, as these are always the same.
- Parameters can be mixed and matched to suit the system's requirements.
- Parameters can be entirely ommitted if not required.
- Avoids writing `read().await` and `write().await` over and over.
- Avoids importing all necessary types/structs/etc. We compile to fully qualified paths.

Jump to the [components](#components) section for a full list of supported system parameters.

### Startup systems

In this phase, components are created and registered into their appropriate manager instance.

```rust
app.startup(system_func);
app.add_system(Phase::Startup, system_instance);
```

### Analyze systems

In this phase, registered components are optionally updated based on the results of an analysis.

```rust
app.analyze(system_func);
app.add_system(Phase::Analyze, system_instance);
```

### Execute systems

In this phase, systems are processed using components to drive business logic. Ideally by this
phase, all components are accessed immutably, but not a hard requirement.

```rust
app.execute(system_func);
app.add_system(Phase::Execute, system_instance);
```

### Shutdown systems

Shutdown runs on successful execution, or on a failure from any phase, and can be used to clean or
reset the current environment, dump error logs or reports, so on and so forth.

```rust
app.shutdown(system_func);
app.add_system(Phase::Shutdown, system_instance);
```

# Components

Components are values that live for the duration of the application (`'static`) and are stored
internally as `Any` instances, ensuring strict uniqueness. Components are dividied into 3
categories:

- States - Granular values.
- Resources - Compound values / singleton instances.
- Emitters - Per-event emitters.

## States

States are components that represent granular pieces of data, are typically implemented with a tuple
or unit struct, and must derive `State`. For example, say we want to track the workspace root.

```rust
use starship::State;
use std::path::PathBuf;

#[derive(Debug, State)]
pub struct WorkspaceRoot(PathBuf);
```

> The `State` derive macro automatically implements `AsRef`, `Deref`, and `DerefMut` when
> applicable. In the future, we may implement other traits deemed necessary.

### Adding state

States can be added directly to the application instance (before the run cycle has started), or
through the `StatesMut` system parameter.

```rust
app.set_state(WorkspaceRoot(PathBuf::from("/")));
```

```rust
#[system]
async fn detect_root(states: StatesMut) {
  states.set(WorkspaceRoot(PathBuf::from("/")));
}
```

### Readable state

The `StatesRef` system parameter can be used to acquire read access to the entire states manager. It
_cannot_ be used alongside `StatesMut`, `StateRef`, or `StateMut`.

```rust
#[system]
async fn read_states(states: StatesRef) {
  let workspace_root = states.get::<WorkspaceRoot>();
}
```

Alternatively, the `StateRef` system parameter can be used to immutably read an individual value
from the states manager. Multiple `StateRef`s can be used together, but cannot be used with
`StateMut`.

```rust
#[system]
async fn read_states(workspace_root: StateRef<WorkspaceRoot>, project: StateRef<Project>) {
  let project_root = workspace_root.join(project.source);
}
```

### Writable state

The `StatesMut` system parameter can be used to acquire write access to the entire states manager.
It _cannot_ be used alongside `StatesRef`, `StateRef` or `StateMut`.

```rust
#[system]
async fn write_states(states: StatesMut) {
  states.set(SomeState);
  states.set(AnotherState);
}
```

Furthermore, the `StateMut` system parameter can be used to mutably access an individual value,
allowing for the value (or its inner value) to be modified. Only 1 `StateMut` can be used in a
system, and no other state related system parameters can be used.

```rust
#[system]
async fn write_state(touched_files: StateMut<TouchedFiles>) {
  touched_files.push(another_path);
}
```

## Resources

Resources are components that represent compound data structures as complex structs, and are akin to
instance singletons in other languages. Some examples of resources are project graphs, dependency
trees, plugin registries, cache engines, etc.

Every resource must derive `Resource`.

```rust
use starship::Resource;
use std::path::PathBuf;

#[derive(Debug, Resource)]
pub struct ProjectGraph {
  pub nodes; // ...
  pub edges; // ...
}
```

> The `Resource` derive macro automatically implements `AsRef`. In the future, we may implement
> other traits deemed necessary.

### Adding resources

Resources can be added directly to the application instance (before the run cycle has started), or
through the `ResourcesMut` system parameter.

```rust
app.set_resource(ProjectGraph::new());
```

```rust
#[system]
async fn create_graph(resources: ResourcesMut) {
  resources.set(ProjectGraph::new());
}
```

### Readable resources

The `ResourcesRef` system parameter can be used to acquire read access to the entire resources
manager. It _cannot_ be used alongside `ResourcesMut`, `ResourceRef`, or `ResourceMut`.

```rust
#[system]
async fn read_resources(resources: ResourcesRef) {
  let project_graph = resources.get::<ProjectGraph>();
}
```

Alternatively, the `ResourceRef` system parameter can be used to immutably read an individual value
from the resources manager. Multiple `ResourceRef`s can be used together, but cannot be used with
`ResourceMut`.

```rust
#[system]
async fn read_resources(project_graph: ResourceRef<ProjectGraph>, cache: ResourceRef<CacheEngine>) {
  let projects = project_graph.load_from_cache(cache).await?;
}
```

### Writable resources

The `ResourcesMut` system parameter can be used to acquire write access to the entire resources
manager. It _cannot_ be used alongside `ResourcesRef`, `ResourceRef` or `ResourceMut`.

```rust
#[system]
async fn write_resources(resources: ResourcesMut) {
  resources.set(ProjectGraph::new());
  resources.set(CacheEngine::new());
}
```

Furthermore, the `ResourceMut` system parameter can be used to mutably access an individual value.
Only 1 `ResourceMut` can be used in a system, and no other resource related system parameters can be
used.

```rust
#[system]
async fn write_resource(cache: ResourceMut<CacheEngine>) {
  let item = cache.load_hash(some_hash).await?;
}
```

## Emitters

Emitters are components that can dispatch events to all registered listeners, allowing for
non-coupled layers to interact with each other. Unlike states and resources that are implemented and
registered individually, emitters are pre-built and provided by the starship `Emitter` struct, and
instead the individual events themselves are implemented.

Events must derive `Event`, or implement the `Event` trait. Events can be any type of struct, but
the major selling point is that events are _mutable_, allowing inner content to be modified by
listeners.

```rust
use starship::{Event, Emitter};
use app::Project;

#[derive(Debug, Event)]
pub struct ProjectCreatedEvent(pub Project);

let emitter = Emitter::<ProjectCreatedEvent>::new();
```

> Jump to the [how to section to learn more about emitting events](#event-emitting).

### Adding emitters

Emitters can be added directly to the application instance (before the run cycle has started), or
through the `EmittersMut` system parameter.

Each emitter represents a singular event, so the event type must be explicitly declared as a generic
when creating a new emitter.

```rust
app.set_emitter(Emitter::<ProjectCreatedEvent>::new());
```

```rust
#[system]
async fn create_emitter(emitters: EmittersMut) {
  emitters.set(Emitter::<ProjectCreatedEvent>::new());
}
```

### Readable emitters

Every method on `Emitter` requires a mutable self, so no system parameters exist for immutably
reading an emitter.

### Writable emitters

The `EmittersMut` system parameter can be used to acquire write access to the entire emitters
manager, where new emitters can be registered, or existing emitters can emit an event. It _cannot_
be used alongside `EmitterMut`.

```rust
#[system]
async fn write_emitters(emitters: EmittersMut) {
  // Add emitter
  emitters.set(Emitter::<ProjectCreatedEvent>::new());

  // Emit event
  emitters.get_mut::<Emitter<ProjectCreatedEvent>().emit(ProjectCreatedEvent::new()).await?;

  // Emit event shorthand
  emitters.emit(ProjectCreatedEvent::new()).await?;
}
```

Furthermore, the `EmitterMut` system parameter can be used to mutably access an individual emitter.
Only 1 `EmitterMut` can be used in a system, and no other emitter related system parameters can be
used.

```rust
#[system]
async fn write_emitter(project_created: EmitterMut<ProjectCreatedEvent>) {
  project_created.emit(ProjectCreatedEvent::new()).await?;
}
```

# How to

## Event emitting

### Using listeners

Listeners are async functions or structs that implement `Listener`, are registered into an emitter,
and are executed when an `Emitter` emits an event. They are passed the event object as a mutable
parameter, allowing for the inner data to be modified.

```rust
use starship::{EventResult, EventState};

async fn listener(event: &mut ProjectCreatedEvent) -> EventResult<ProjectCreatedEvent> {
  event.0.root = new_path;
  Ok(EventState::Continue)
}

// TODO: These currently don't work because of lifetime issues!
emitter.on(listener); // Runs multiple times
emitter.once(listener); // Only runs once
```

```rust
use starship::{EventResult, EventState, Listener};
use async_trait::async_trait;

struct TestListener;

#[async_trait]
impl Listener<ProjectCreatedEvent> for TestListener {
  fn is_once(&self) -> bool {
    false
  }

  async fn on_emit(&mut self, event: &mut ProjectCreatedEvent) -> EventResult<ProjectCreatedEvent> {
    event.0.root = new_path;
    Ok(EventState::Continue)
  }
}

emitter.listen(TestListener);
```

As a temporary solution for async function lifetime issues, we provide `#[listener]` and
`#[listener(once)]` function attributes, which will convert the function to a `Listener` struct
internally. The major drawback is that the function name is lost, and the new struct name must be
passed to `listen()`.

```rust
use starship::{EventResult, EventState, listener};

#[listener]
async fn some_func(event: &mut ProjectCreatedEvent) -> EventResult<ProjectCreatedEvent> {
  event.0.root = new_path;
  Ok(EventState::Continue)
}

// Rename to...
emitter.listen(SomeFuncListener);
```

### Controlling the event flow

Listeners can control this emit execution flow by returning `EventState`, which supports the
following variants:

- `Continue` - Continues to the next listener.
- `Stop` - Stops after this listener, discarding subsequent listeners.
- `Return` - Like `Stop` but also returns a value for interception.

```rust
async fn continue_flow(event: &mut CacheCheckEvent) -> EventResult<CacheCheckEvent> {
  Ok(EventState::Continue)
}

async fn stop_flow(event: &mut CacheCheckEvent) -> EventResult<CacheCheckEvent> {
  Ok(EventState::Stop)
}

async fn return_flow(event: &mut CacheCheckEvent) -> EventResult<CacheCheckEvent> {
  Ok(EventState::Return(path_to_cache)))
}
```

For `Return` flows, the type of value returned is inferred from the event. By default the value is a
unit type (`()`), but can be customized with `#[event]` or `type Value` when implementing manually.

```rust
use starship::{Event, Emitter};
use std::path::PathBuf;

#[derive(Event)]
#[event(value = "PathBuf")]
pub struct CacheCheckEvent(pub PathBuf);

// OR
pub struct CacheCheckEvent(pub PathBuf);

impl Event for CacheCheckEvent {
  type Value = PathBuf;
}
```

### Emitting and results

When an event is emitted, listeners are executed sequentially in the same thread so that each
listener can mutate the event if necessary. Because of this, events do not support references for
inner values, and instead must own everything.

An event can be emitted with the `emit()` method, which requires an owned event (and owned inner
data).

```rust
let (event, result) = emitters.emit(ProjectCreatedEvent::new(owned_inner_data)).await?;

// Take ownership of inner data
let project = event.0;
```

Emitting returns a tuple, containing the final event after all modifications, and a result of type
`Option<Event::Value>` (which is provided with `EventState::Return`).

## Error handling

Errors and diagnostics are provided by the [`miette`](https://crates.io/crates/miette) crate. All
layers of the application, from systems, to events, and the application itself, return the
`miette::Result` type. This allows for errors to be easily converted to diagnostics, and for miette
to automatically render to the terminal for errors and panics.

To benefit from this, update your `main` function to return `MainResult`.

```rust
use starship::MainResult;

#[tokio::main]
async fn main() -> MainResult {
  let mut app = App::new();
  // ...
  app.run().await?;

  Ok(())
}
```

To make the most out of errors, and in turn diagnostics, it's best (also suggested) to use the
`thiserror` crate.

```rust
use starship::Diagnostic;
use thiserror::Error;

#[derive(Debug, Diagnostic, Error)]
pub enum AppError {
    #[error(transparent)]
    #[diagnostic(code(app::io_error))]
    IoError(#[from] std::io::Error),

    #[error("Systems offline!")]
    #[diagnostic(code(app::bad_code))]
    SystemsOffline,
}
```

### Caveats

In systems, events, and other fallible layers, a returned `Err` must be converted to a diagnostic
first. There are 2 approaches to achieve this:

```rust
#[system]
async fn could_fail() {
  // Convert error using into()
  Err(AppError::SystemsOffline.into())

  // OR use ? operator on Err()
  Err(AppError::SystemsOffline)?
}
```
