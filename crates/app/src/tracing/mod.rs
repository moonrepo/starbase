mod format;
mod level;
#[cfg(feature = "otel")]
mod otel;

use crate::tracing::format::*;
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, atomic::Ordering};
use std::time::SystemTime;
use thiserror::Error;
use tracing::subscriber::set_global_default;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::fmt;
use tracing_subscriber::{EnvFilter, prelude::*};

pub use crate::tracing::level::LogLevel;
#[cfg(feature = "otel")]
pub use crate::tracing::otel::OtelOptions;
pub use tracing::{
    debug, debug_span, enabled, error, error_span, event, event_enabled, info, info_span,
    instrument, span, span_enabled, trace, trace_span, warn, warn_span,
};

/// A result type for tracing setup and configuration.
pub type TracingResult<T> = Result<T, TracingError>;

/// Errors related to tracing setup and configuration.
#[derive(Debug, Error)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum TracingError {
    #[cfg_attr(
        feature = "miette",
        diagnostic(code(app::tracing::create_log_dir_failed))
    )]
    #[error("Failed to create log directory.")]
    CreateLogDirFailed {
        #[source]
        error: std::io::Error,
    },

    #[cfg_attr(
        feature = "miette",
        diagnostic(code(app::tracing::create_log_file_failed))
    )]
    #[error("Failed to create log file.")]
    CreateLogFileFailed {
        #[source]
        error: std::io::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(app::tracing::log_level_invalid)))]
    #[error("Invalid log level: {level}")]
    LogLevelInvalid { level: String },

    #[cfg(feature = "log-compat")]
    #[cfg_attr(
        feature = "miette",
        diagnostic(code(app::tracing::intercept_log_failed))
    )]
    #[error("Failed to initialize log interceptor.")]
    InterceptLogFailed {
        #[source]
        error: tracing_log::log_tracer::SetLoggerError,
    },

    #[cfg(feature = "otel")]
    #[cfg_attr(
        feature = "miette",
        diagnostic(code(app::tracing::otlp_exporter_failed))
    )]
    #[error("Failed to initialize OTLP {signal} exporter.")]
    OtlpExporterFailed {
        signal: String,
        #[source]
        error: opentelemetry_otlp::ExporterBuildError,
    },
}

/// Options for configuring [`tracing`] behavior.
#[derive(Debug)]
pub struct TracingOptions {
    /// Minimum level of messages to display.
    pub default_level: LogLevel,
    /// Dump a trace file that can be viewed in Chrome.
    pub dump_trace: bool,
    /// List of modules/prefixes to only log.
    pub filter_modules: Vec<String>,
    /// Whether to intercept messages from the global `log` crate.
    /// Requires the `log-compat` feature.
    #[cfg(feature = "log-compat")]
    pub intercept_log: bool,
    /// Name of the logging environment variable.
    pub log_env: String,
    /// Absolute path to a file to write logs to.
    pub log_file: Option<PathBuf>,
    /// Whether to output logs in NDJSON format.
    pub ndjson: bool,
    /// OpenTelemetry export settings. Requires the `otel` feature.
    #[cfg(feature = "otel")]
    pub otel: OtelOptions,
    /// Show span hierarchy in log output.
    pub show_spans: bool,
    /// Name of the testing environment variable.
    pub test_env: String,
}

impl Default for TracingOptions {
    fn default() -> Self {
        TracingOptions {
            default_level: LogLevel::Info,
            dump_trace: false,
            filter_modules: vec![],
            #[cfg(feature = "log-compat")]
            intercept_log: true,
            log_env: "STARBASE_LOG".into(),
            log_file: None,
            ndjson: false,
            #[cfg(feature = "otel")]
            otel: OtelOptions::default(),
            show_spans: false,
            test_env: "STARBASE_TEST".into(),
        }
    }
}

/// A guard that flushes and cleans up tracing resources when dropped.
pub struct TracingGuard {
    chrome_guard: Option<FlushGuard>,
    log_file: Option<Arc<File>>,
    #[cfg(feature = "otel")]
    otel_guard: Option<otel::OtelGuard>,
}

#[instrument]
pub fn setup_tracing(options: TracingOptions) -> TracingResult<TracingGuard> {
    TEST_ENV.store(env::var(options.test_env).is_ok(), Ordering::Release);

    // Determine modules to log
    let level = env::var(&options.log_env).unwrap_or_else(|_| options.default_level.to_string());

    unsafe {
        env::set_var(
            &options.log_env,
            if options.filter_modules.is_empty()
                || level == "off"
                || level.contains(',')
                || level.contains('=')
            {
                level
            } else if level == "verbose" {
                "trace".into()
            } else {
                options
                    .filter_modules
                    .iter()
                    .map(|prefix| format!("{prefix}={level}"))
                    .collect::<Vec<_>>()
                    .join(",")
            },
        )
    };

    #[cfg(feature = "log-compat")]
    if options.intercept_log {
        tracing_log::LogTracer::init()
            .map_err(|error| TracingError::InterceptLogFailed { error })?;
    }

    // Build the formatting layer. NDJSON and the console formatter produce
    // different field/event formatter types, so box them into a single layer
    // type that both `if` arms can return.
    let fmt_layer = if options.ndjson {
        fmt::layer()
            .json()
            .with_span_list(false)
            .with_current_span(options.show_spans)
            .with_ansi(false)
            .with_target(true)
            .with_writer(io::stderr)
            .flatten_event(true)
            .boxed()
    } else {
        fmt::layer()
            .with_ansi(true)
            .with_target(true)
            .with_writer(io::stderr)
            .event_format(EventFormatter {
                show_spans: options.show_spans,
            })
            .fmt_fields(FieldFormatter)
            .boxed()
    };

    // Add layers to our subscriber
    let mut guard = TracingGuard {
        chrome_guard: None,
        log_file: None,
        #[cfg(feature = "otel")]
        otel_guard: None,
    };

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_env(options.log_env))
        .with(fmt_layer)
        // Write to a log file
        .with(if let Some(log_file) = options.log_file {
            if let Some(dir) = log_file.parent() {
                fs::create_dir_all(dir)
                    .map_err(|error| TracingError::CreateLogDirFailed { error })?;
            }

            let file = Arc::new(
                File::create(log_file)
                    .map_err(|error| TracingError::CreateLogFileFailed { error })?,
            );

            guard.log_file = Some(Arc::clone(&file));

            Some(
                fmt::layer()
                    .with_ansi(false)
                    .with_target(true)
                    .with_writer(file)
                    .event_format(EventFormatter {
                        show_spans: options.show_spans,
                    })
                    .fmt_fields(FieldFormatter),
            )
        } else {
            None
        })
        // Dump a trace profile
        .with(if options.dump_trace {
            let (chrome_layer, chrome_guard) = ChromeLayerBuilder::new()
                .include_args(true)
                .include_locations(true)
                .file(format!(
                    "./dump-{}.json",
                    SystemTime::UNIX_EPOCH.elapsed().unwrap().as_micros()
                ))
                .build();

            guard.chrome_guard = Some(chrome_guard);

            Some(chrome_layer)
        } else {
            None
        });

    #[cfg(feature = "otel")]
    let subscriber = {
        let (subscriber, otel_guard) = otel::extend_subscriber(subscriber, &options.otel)?;
        guard.otel_guard = Some(otel_guard);
        subscriber
    };

    let _ = set_global_default(subscriber);

    Ok(guard)
}
