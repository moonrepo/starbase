use chrono::{Local, Timelike};
use starbase_styles::color;
use std::env;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tracing::metadata::LevelFilter;
use tracing::{field::Visit, subscriber::set_global_default, Level, Metadata, Subscriber};
use tracing_subscriber::{
    field::RecordFields,
    fmt::{self, time::FormatTime, FormatEvent, FormatFields, SubscriberBuilder},
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
            write!(self.writer, "  {:?} ", value).unwrap()
        // } else if !TEST_ENV.load(Ordering::Relaxed) {
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

struct EventFormatter;

impl FormatTime for EventFormatter {
    fn format_time(&self, writer: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        if TEST_ENV.load(Ordering::Relaxed) {
            return write!(writer, "YYYY-MM-DD");
        }

        let mut date_format = "%Y-%m-%d %H:%M:%S";
        let current_timestamp = Local::now();
        let current_hour = current_timestamp.hour() as u8;

        if current_hour == LAST_HOUR.load(Ordering::Acquire) {
            date_format = "%H:%M:%S";
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

        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                write!(
                    writer,
                    "{}{}",
                    color::muted(":"),
                    color::muted_light(span.name())
                )?;
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

pub struct TracingOptions {
    /// Minimum level of messages to display.
    pub default_level: LevelFilter,
    /// List of modules/prefixes to only log.
    pub filter_modules: Vec<String>,
    /// Whether to intercept messages from the global `log` crate.
    pub intercept_log: bool,
    /// Name of the logging environment variable.
    pub log_env: String,
    /// Absolute path to a file to write logs to.
    pub log_file: Option<PathBuf>,
    /// Name of the testing environment variable.
    pub test_env: String,
}

impl Default for TracingOptions {
    fn default() -> Self {
        TracingOptions {
            default_level: LevelFilter::INFO,
            filter_modules: vec![],
            intercept_log: true,
            log_env: "RUST_LOG".into(),
            log_file: None,
            test_env: "STARBASE_TEST".into(),
        }
    }
}

pub fn setup_tracing(options: TracingOptions) {
    TEST_ENV.store(env::var(options.test_env).is_ok(), Ordering::Release);

    let set_env_var = |level: String| {
        let env_value = if options.filter_modules.is_empty() {
            level
        } else {
            options
                .filter_modules
                .iter()
                .map(|prefix| format!("{prefix}={level}"))
                .collect::<Vec<_>>()
                .join(",")
        };

        env::set_var(&options.log_env, env_value);
    };

    if let Ok(level) = env::var(&options.log_env) {
        if !level.contains('=') && !level.contains(',') && level != "off" {
            set_env_var(level);
        }
    } else {
        set_env_var(options.default_level.to_string().to_lowercase());
    }

    if options.intercept_log {
        tracing_log::LogTracer::init().expect("Failed to initialize log interceptor.");
    }

    let subscriber = SubscriberBuilder::default()
        .event_format(EventFormatter)
        .fmt_fields(FieldFormatter)
        .with_env_filter(EnvFilter::from_env(options.log_env));

    // Ignore the error in case the subscriber is already set
    let _ = if let Some(log_file) = options.log_file {
        env::set_var("NO_COLOR", "1");

        set_global_default(
            subscriber
                .with_writer(tracing_appender::rolling::never(
                    log_file
                        .parent()
                        .expect("Missing parent directory for log file."),
                    log_file
                        .file_name()
                        .expect("Missing file name for log file."),
                ))
                .finish(),
        )
    } else {
        set_global_default(subscriber.with_writer(io::stderr).finish())
    };
}
