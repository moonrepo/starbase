# starship

Starship is a framework for building performant command line applications or processing pipelines.
It takes heavy inspiration from the popular
[ECS pattern](https://en.wikipedia.org/wiki/Entity_component_system) but works quite differently.

A starship is built with the following modules:

- **Reactor core** - Async-first powered by the `tokio` runtime.
- **Warp drive** - Thread-safe concurrent systems for easy processing.
- **Communication array** - Event-driven architecture to decouple and isolate crates.
- **Shield generator** - ???
- **Navigation sensors** -

### Roadmap

- [x] Async application layer built on `tokio`
  - [x] Systems
  - [x] States/resources
  - [x] Event emitters
- [ ] Logging + tracing via the `tracing` crate
  - [ ] Include `metrics`?
- [ ] Error handling + diagnostics via the `miette` crate
  - [ ] Replace `anyhow`

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
use starship::{States, Resources, Emitters, SystemResult};

async fn load_config(states: States, resources: Resources, emitters: Emitters) -> SystemResult {
	let states = states.write().await;

	let config: AppConfig = do_load_config();
	states.set::<AppConfig>(config);

	Ok(())
}

#[tokio::main]
async fn main() {
	let mut app = App::new();
	app.startup(load_config);
	app.run()?;
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

## States

## Resources

## Emitters
