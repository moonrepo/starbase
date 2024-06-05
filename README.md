# Starbase

![Crates.io](https://img.shields.io/crates/v/starbase)
![Crates.io](https://img.shields.io/crates/d/starbase)

Starbase is a framework, a collection of crates, for building performant command line based
developer tools. Starbase is CLI agnostic and can be used with clap, structopt, or another library
of your choice.

A starbase is built with the following modules:

- **Reactor core** - Async-first powered by the `tokio` runtime.
- **Fusion cells** - Thread-safe concurrent systems for easy processing.
- **Communication array** - Event-driven architecture with
  [`starbase_events`](https://crates.io/crates/starbase_events).
- **Shield generator** - Native diagnostics and reports with `miette`.
- **Navigation sensors** - Span based instrumentation and logging with `tracing`.
- **Engineering bay** - Ergonomic utilities with
  [`starbase_utils`](https://crates.io/crates/starbase_utils).
- **Command center** - Terminal styling and theming with
  [`starbase_styles`](https://crates.io/crates/starbase_styles).
- **Operations drive** - Shell detection and profile management with
  [`starbase_shell`](https://crates.io/crates/starbase_shell).
- **Cargo hold** - Archive packing and unpacking with
  [`starbase_archive`](https://crates.io/crates/starbase_archive).
