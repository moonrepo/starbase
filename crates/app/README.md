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
use std::process::ExitCode;
use crate::CustomSession;

#[tokio::main]
async fn main() -> MainResult {
  let app = App::default();
  app.setup_diagnostics();

  let outcome = app.run(CustomSession::default(), |session| async {
    // Run CLI
    Ok(None)
  }).await;

  // `run` returns an `AppRunOutcome`; `into_result()` collapses it into a
  // `Result<u8, E>` so the error (if any) and exit code can bubble up.
  Ok(ExitCode::from(outcome.into_result()?))
}
```

## Session

A session must implement the `AppSession` trait. This trait provides an associated `Error` type and
4 optional methods, each representing a different [phase](#phases) in the application life cycle.

The `Error` type can be anything that implements `Debug + Display + Send + 'static`. This includes
concrete `std::error::Error` types, `Box<dyn Error>`, and the type-erased reporters `anyhow::Error`
and `miette::Report`. Each phase returns `AppResult<Self::Error>`, an alias for
`Result<Option<u8>, Self::Error>`, where the optional `u8` is an exit code.

```rust
use starbase::{AppSession, AppResult};
use miette::Report;
use std::path::PathBuf;
use async_trait::async_trait;

#[derive(Clone)]
pub struct CustomSession {
  pub workspace_root: PathBuf,
}

#[async_trait]
impl AppSession for CustomSession {
  // Use any error type that is `Debug + Display + Send + 'static`.
  type Error = Report;

  async fn startup(&mut self) -> AppResult<Self::Error> {
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

Each session chooses its own error type through the `AppSession::Error` associated type, so starbase
itself is not tied to any specific error library. The only requirement is that the type implements
`Debug + Display + Send + 'static`.

miette remains the default diagnostics and panic reporter: `App::setup_diagnostics` registers
miette's hooks, and `MainResult` is a `miette::Result<ExitCode>`. Because `miette::Report` satisfies
the error bound, it's the recommended error type when you want rich diagnostics rendered to the
terminal. Note that `Report` intentionally does _not_ implement `std::error::Error`, which is why
the bound is `Display`-based rather than `Error`-based. miette is also re-exported as
`starbase::diagnostics` for convenience.

`App::run` returns an `AppRunOutcome`, which carries the last phase, an optional error, and the
resolved exit code. Call `into_result()` to collapse it into a `Result<u8, Self::Error>` for your
`main` function.

```rust
use starbase::{App, MainResult};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> MainResult {
  let app = App::default();
  app.setup_diagnostics();

  let _guard = app.setup_tracing_with_defaults()?;

  let outcome = app.run(CustomSession::default(), |session| async {
    // Run CLI
    Ok(None)
  }).await;

  outcome.into_exit_result()
}
```

## OpenTelemetry tracing

When the `otel` feature is enabled, `TracingOptions` can also export traces and metrics over OTLP.

```rust
use starbase::tracing::{OtelOptions, TracingOptions};

let _guard = app.setup_tracing(TracingOptions {
    otel: OtelOptions {
        enabled: true,
        logs_enabled: false,
        service_name: Some("my-app".into()),
    },
    ..TracingOptions::default()
})?;
```

This wires the OTLP tracing and metrics bridge. Endpoint, protocol, headers, and other exporter
behavior should still be configured through the standard OpenTelemetry environment variables.

## Custom error types

To make the most out of errors, and in turn diagnostics, it's best (also suggested) to use the
`thiserror` crate to define a concrete error type, optionally deriving miette's `Diagnostic` for
rich output.

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

You can then use this type directly as your session's error, and return failures from any phase with
the `?` operator or by constructing the error.

```rust
#[async_trait]
impl AppSession for CustomSession {
    type Error = AppError;

    async fn startup(&mut self) -> AppResult<Self::Error> {
        // Bubble up with `?`...
        could_fail()?;

        // ...or return the error directly.
        Err(AppError::SystemsOffline)
    }
}
```

> If you instead set `type Error = miette::Report`, convert errors with `.into()` or the `?`
> operator, which turns any `Diagnostic` into a `Report`.
