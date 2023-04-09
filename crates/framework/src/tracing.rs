use chrono::{Local, Timelike};
use starbase_styles::color;
use std::env;
use std::sync::atomic::{AtomicU8, Ordering};
use tracing::{field::Visit, Level, Metadata, Subscriber};
// use tracing_subscriber::fmt::FormattedFields;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{
    field::RecordFields,
    fmt::{self, time::FormatTime, FormatEvent, FormatFields, SubscriberBuilder},
    registry::LookupSpan,
};

static LAST_HOUR: AtomicU8 = AtomicU8::new(0);

struct FieldVisitor<'writer> {
    is_testing: bool,
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
        } else if !self.is_testing {
            write!(
                self.writer,
                " {}",
                color::muted(format!("{}={:?}", field.name(), value))
            )
            .unwrap()
        }
    }
}

struct FieldFormatter {
    is_testing: bool,
}

impl FieldFormatter {
    pub fn new() -> Self {
        Self {
            is_testing: env::var("STARBASE_TEST").is_ok(),
        }
    }
}

impl<'writer> FormatFields<'writer> for FieldFormatter {
    fn format_fields<R: RecordFields>(
        &self,
        writer: fmt::format::Writer<'writer>,
        fields: R,
    ) -> std::fmt::Result {
        let mut visitor = FieldVisitor {
            is_testing: self.is_testing,
            writer,
        };

        fields.record(&mut visitor);

        Ok(())
    }
}

struct EventFormatter {
    is_testing: bool,
}

impl EventFormatter {
    pub fn new() -> Self {
        Self {
            is_testing: env::var("STARBASE_TEST").is_ok(),
        }
    }
}

impl FormatTime for EventFormatter {
    fn format_time(&self, writer: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        if self.is_testing {
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

        // [level timestamp]
        write!(writer, "{}", color::muted("["))?;
        write!(
            writer,
            "{} ",
            color::muted(format!("{: >5}", level.as_str()))
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

pub fn set_tracing_subscriber(env_name: &str) {
    let subscriber = SubscriberBuilder::default()
        .event_format(EventFormatter::new())
        .fmt_fields(FieldFormatter::new())
        .with_env_filter(EnvFilter::from_env(env_name))
        .finish();

    // Ignore the error incase the subscriber is already set
    let _ = tracing::subscriber::set_global_default(subscriber);

    // tracing_subscriber::fmt::init();
}
