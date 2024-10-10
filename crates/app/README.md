# starbase

![Crates.io](https://img.shields.io/crates/v/starbase)
![Crates.io](https://img.shields.io/crates/d/starbase)

Application framework for building performant command line applications and developer tools.

# Usage

An application uses a session based approach, where a session object contains data required for the
entire application lifecycle.

Create an `App`, optionally setup diagnostics (`miette`) and tracing (`tracing`), and then run the
application with the provided session. A mutable session is required, as the session can be mutated
for each [phase](#phases).

```rust
use starbase::{App, MainResult};
use crate::CustomSession;

#[tokio::main]
async fn main() -> MainResult {
  let app = App::default();
  app.setup_diagnostics();

  let exit_code = app.run(CustomSession::default(), |session| async {
    // Run CLI
    Ok(None)
  }).await?;

  if exit_code > 0 {
    std::process::exit(exit_code);
  }

  Ok(())
}
```

## Session

A session must implement the `AppSession` trait. This trait provides 4 optional methods, each
representing a different [phase](#phases) in the application life cycle.

```rust
use starbase::{AppSession, AppResult};
use std::path::PathBuf;
use async_trait::async_trait;

#[derive(Clone)]
pub struct CustomSession {
  pub workspace_root: PathBuf,
}

#[async_trait]
impl AppSession for CustomSession {
  async fn startup(&mut self) -> AppResult {
    self.workspace_root = detect_workspace_root()?;
    Ok(None)
  }
}
```

> Sessions _must be_ cloneable _and be_ `Send + Sync` compatible. We clone the session when spawning
> tokio tasks. If you want to persist data across threads, wrap session properties in `Arc`,
> `RwLock`, and other mechanisms.

## Phases

An application is divided into phases, where each phase will be processed and completed before
moving onto the next phase. The following phases are available:

- **Startup** - Register, setup, or load initial session state.
  - Example: load configuration, detect workspace root, load plugins
- **Analyze** - Analyze the current environment, update state, and prepare for execution.
  - Example: generate project graph, load cache, signin to service
- **Execute** - Execute primary business logic (`App#run`).
  - Example: process dependency graph, run generator, check for new version
- **Shutdown** - Cleanup and shutdown on success of the entire lifecycle, or on failure of a
  specific phase.
  - Example: cleanup temporary files, shutdown server

> If a session implements the `AppSession#execute` trait method, it will run in parallel with the
> `App#run` method.

# How to

## Error handling

Errors and diagnostics are provided by the [`miette`](https://crates.io/crates/miette) crate. All
layers of the application return the `miette::Result` type (via `AppResult`). This allows for errors
to be easily converted to diagnostics, and for miette to automatically render to the terminal for
errors and panics.

To benefit from this, update your `main` function to return `MainResult`.

```rust
use starbase::{App, MainResult};

#[tokio::main]
async fn main() -> MainResult {
  let app = App::default();
  app.setup_diagnostics();
  app.setup_tracing_with_defaults();

  // ...

  Ok(())
}
```

To make the most out of errors, and in turn diagnostics, it's best (also suggested) to use the
`thiserror` crate.

```rust
use miette::Diagnostic;
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

A returned `Err` must be converted to a diagnostic first. There are 2 approaches to achieve this:

```rust
#[system]
async fn could_fail() {
  // Convert error using into()
  Err(AppError::SystemsOffline.into())

  // OR use ? operator on Err()
  Err(AppError::SystemsOffline)?
}
```
