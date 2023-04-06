# starbase_events

![Crates.io](https://img.shields.io/crates/v/starbase_events)
![Crates.io](https://img.shields.io/crates/d/starbase_events)

An async event emitter for the `starbase` application framework. This crate works quite differently
than other event systems, as subscribers _can mutate_ the event and its data. Because of this, we
cannot use message channels, and must take extra precaution to satisfy `Send` + `Sync` requirements.

## Creating events

Events must derive `Event`, or implement the `Event` trait. Events can be any type of struct, but
the major selling point is that events are _mutable_, allowing inner content to be modified by
subscribers.

```rust
use starbase_events::Event;
use app::Project;

#[derive(Debug, Event)]
pub struct ProjectCreatedEvent(pub Project);
```

## Creating emitters

An `Emitter` is in charge of managing subscribers, and dispatching an event to each subscriber,
while taking into account the execution flow and once subscribers.

Every event will require its own emitter instance.

```rust
use starbase_events::Emitter;

let project_created = Emitter::<ProjectCreatedEvent>::new();
let cache_check: Emitter<CacheCheckEvent> = Emitter::new();
```

## Using subscribers

Subscribers are async functions that are registered into an emitter, and are executed when the
emitter emits an event. They are passed the event object as a `Arc<RwLock<T>>`, allowing for the
event and its inner data to be accessed mutably or immutably.

```rust
use starbase_events::{EventResult, EventState};

async fn update_root(event: Arc<RwLock<ProjectCreatedEvent>>) -> EventResult<ProjectCreatedEvent> {
  let event = event.write().await;

  event.0.root = new_path;

  Ok(EventState::Continue)
}

emitter.on(subscriber).await; // Runs multiple times
emitter.once(subscriber).await; // Only runs once
```

Furthermore, we provide a `#[subscriber]` function attribute that streamlines the function
implementation. For example, the above subscriber can be rewritten as:

```rust
#[subscriber]
async fn update_root(mut event: ProjectCreatedEvent) {
  event.0.root = new_path;
}
```

When using `#[subscriber]`, the following benefits apply:

- The return type is optional.
- The return value is optional if `EventState::Continue`.
- Using `mut event` or `&mut Event` will acquire a write lock, otherwise a read lock.
- Omitting the event parameter will not acquire any lock.

## Controlling the event flow

Subscribers can control the event execution flow by returning `EventState`, which supports the
following variants:

- `Continue` - Continues to the next subscriber (default).
- `Stop` - Stops after this subscriber, discarding subsequent subscribers.
- `Return` - Like `Stop` but also returns a value for interception.

```rust
#[subscriber]
async fn continue_flow(mut event: CacheCheckEvent) {
  Ok(EventState::Continue)
}

#[subscriber]
async fn stop_flow(mut event: CacheCheckEvent) {
  Ok(EventState::Stop)
}

#[subscriber]
async fn return_flow(mut event: CacheCheckEvent) {
  Ok(EventState::Return(path_to_cache)))
}
```

For `Return` flows, the type of value returned is inferred from the event. By default the value is a
unit type (`()`), but can be customized with `#[event]` for derived events, or `type Value` when
implemented manually.

```rust
use starbase_events::{Event, Emitter};
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

## Emitting and handling results

When an event is emitted, subscribers are executed sequentially in the same thread so that each
subscriber can mutate the event if necessary. Because of this, events do not support references for
inner values, and instead must own everything.

An event can be emitted with the `emit()` method, which requires an owned event (and owned inner
data).

```rust
let (event, result) = emitter.emit(ProjectCreatedEvent(owned_project)).await?;

// Take back ownership of inner data
let project = event.0;
```

Emitting returns a tuple, containing the final event after all modifications, and a result of type
`Option<Event::Value>` (which is provided with `EventState::Return`).
