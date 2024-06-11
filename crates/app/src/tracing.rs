use chrono::{Local, Timelike};
use starbase_styles::color;
use starbase_styles::color::apply_style_tags;
use std::env;
use std::fmt as stdfmt;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{
    field::Visit, metadata::LevelFilter, subscriber::set_global_default, Level, Metadata,
    Subscriber,
};
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::{
    field::RecordFields,
    fmt::{self, time::FormatTime, FormatEvent, FormatFields, SubscriberBuilder},
    prelude::*,
    registry::LookupSpan,
    EnvFilter,
};

pub use tracing::{
    debug, debug_span, enabled, error, error_span, event, event_enabled, info, info_span,
    instrument, span, span_enabled, trace, trace_span, warn, warn_span,
};

static LAST_HOUR: AtomicU8 = AtomicU8::new(0);
static TEST_ENV: AtomicBool = AtomicBool::new(false);

struct FieldVisitor<'writer> {
    writer: fmt::format::Writer<'writer>,
}

impl<'writer> Visit for FieldVisitor<'writer> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{}", value))
        } else {
            self.record_debug(field, &value)
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            write!(
                self.writer,
                "  {} ",
                apply_style_tags(format!("{:?}", value))
            )
            .unwrap()
        } else {
            write!(
                self.writer,
                " {}",
                color::muted(format!("{}={:?}", field.name(), value))
            )
            .unwrap()
        }
    }
}

struct FieldFormatter;

impl<'writer> FormatFields<'writer> for FieldFormatter {
    fn format_fields<R: RecordFields>(
        &self,
        writer: fmt::format::Writer<'writer>,
        fields: R,
    ) -> std::fmt::Result {
        let mut visitor = FieldVisitor { writer };

        fields.record(&mut visitor);

        Ok(())
    }
}

struct EventFormatter {
    show_spans: bool,
}

impl FormatTime for EventFormatter {
    fn format_time(&self, writer: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        if TEST_ENV.load(Ordering::Relaxed) {
            return write!(writer, "YYYY-MM-DD");
        }

        let mut date_format = "%Y-%m-%d %H:%M:%S%.3f";
        let current_timestamp = Local::now();
        let current_hour = current_timestamp.hour() as u8;

        if current_hour == LAST_HOUR.load(Ordering::Acquire) {
            date_format = "%H:%M:%S%.3f";
        } else {
            LAST_HOUR.store(current_hour, Ordering::Release);
        }

        write!(
            writer,
            "{}",
            color::muted(current_timestamp.format(date_format).to_string()),
        )
    }
}

impl<S, N> FormatEvent<S, N> for EventFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &fmt::FmtContext<'_, S, N>,
        mut writer: fmt::format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let meta: &Metadata = event.metadata();
        let level: &Level = meta.level();
        let level_label = format!("{: >5}", level.as_str());

        // [level timestamp]
        write!(writer, "{}", color::muted("["))?;
        write!(
            writer,
            "{} ",
            if *level == LevelFilter::ERROR {
                color::failure(level_label)
            } else if *level == LevelFilter::WARN {
                color::invalid(level_label)
            } else {
                color::muted(level_label)
            }
        )?;

        self.format_time(&mut writer)?;

        write!(writer, "{}", color::muted("]"))?;

        // target:spans...
        write!(writer, " {}", color::log_target(meta.target()))?;

        if self.show_spans {
            write!(writer, " ")?;

            if let Some(scope) = ctx.event_scope() {
                for span in scope.from_root() {
                    if span.parent().is_some() {
                        write!(writer, "{}", color::muted(":"))?;
                    }

                    write!(writer, "{}", color::muted_light(span.name()))?;
                }
            }
        }

        // message ...field=value
        ctx.format_fields(writer.by_ref(), event)?;

        // spans(vars=values)...
        // if let Some(scope) = ctx.event_scope() {
        //     for span in scope.from_root() {
        //         let ext = span.extensions();

        //         if let Some(fields) = &ext.get::<FormattedFields<N>>() {
        //             write!(
        //                 writer,
        //                 " {}{}{}{}",
        //                 color::muted_light(span.name()),
        //                 color::muted_light("("),
        //                 fields,
        //                 color::muted_light(")"),
        //             )?;
        //         } else {
        //             write!(writer, " {}", color::muted_light(span.name()))?;
        //         }
        //     }
        // }

        writeln!(writer)
    }
}

#[derive(Clone, Debug, Default)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl stdfmt::Display for LogLevel {
    fn fmt(&self, f: &mut stdfmt::Formatter<'_>) -> Result<(), stdfmt::Error> {
        write!(
            f,
            "{}",
            match self {
                LogLevel::Off => "off",
                LogLevel::Error => "error",
                LogLevel::Warn => "warn",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
                LogLevel::Trace => "trace",
            }
        )
    }
}

impl From<String> for LogLevel {
    fn from(value: String) -> Self {
        match value.to_lowercase().as_str() {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            _ => Self::Off,
        }
    }
}

impl FromStr for LogLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(value.to_owned()))
    }
}

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
            || level.contains(",")
            || level.contains("=")
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
