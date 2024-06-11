mod format;
mod level;

use crate::tracing::format::*;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;
use std::{env, fs};
use tracing::subscriber::set_global_default;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::fmt::{self, SubscriberBuilder};
use tracing_subscriber::{prelude::*, EnvFilter};

pub use crate::tracing::level::LogLevel;
pub use tracing::{
    debug, debug_span, enabled, error, error_span, event, event_enabled, info, info_span,
    instrument, span, span_enabled, trace, trace_span, warn, warn_span,
};

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
            show_spans: false,
            test_env: "STARBASE_TEST".into(),
        }
    }
}

pub struct TracingGuard {
    chrome_guard: Option<FlushGuard>,
    log_file: Option<Arc<File>>,
}

#[tracing::instrument(skip_all)]
pub fn setup_tracing(options: TracingOptions) -> TracingGuard {
    TEST_ENV.store(env::var(options.test_env).is_ok(), Ordering::Release);

    // Determine modules to log
    let level = env::var(&options.log_env).unwrap_or_else(|_| options.default_level.to_string());

    env::set_var(
        &options.log_env,
        if options.filter_modules.is_empty()
            || level == "off"
            || level.contains(',')
            || level.contains('=')
        {
            level
        } else {
            options
                .filter_modules
                .iter()
                .map(|prefix| format!("{prefix}={level}"))
                .collect::<Vec<_>>()
                .join(",")
        },
    );

    #[cfg(feature = "log-compat")]
    if options.intercept_log {
        tracing_log::LogTracer::init().expect("Failed to initialize log interceptor.");
    }

    // Build our subscriber
    let subscriber = SubscriberBuilder::default()
        .event_format(EventFormatter {
            show_spans: options.show_spans,
        })
        .fmt_fields(FieldFormatter)
        .with_env_filter(EnvFilter::from_env(options.log_env))
        .with_writer(io::stderr)
        .finish();

    // Add layers to our subscriber
    let mut guard = TracingGuard {
        chrome_guard: None,
        log_file: None,
    };

    let _ = set_global_default(
        subscriber
            // Write to a log file
            .with(if let Some(log_file) = options.log_file {
                if let Some(dir) = log_file.parent() {
                    fs::create_dir_all(dir).expect("Failed to create log directory.");
                }

                let file = Arc::new(File::create(log_file).expect("Failed to create log file."));

                guard.log_file = Some(Arc::clone(&file));

                Some(fmt::layer().with_ansi(false).with_writer(file))
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
            }),
    );

    guard
}
