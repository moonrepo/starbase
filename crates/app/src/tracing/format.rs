use chrono::{Local, Timelike};
use starbase_styles::{apply_style_tags, color};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tracing::{Level, Metadata, Subscriber, field::Visit, metadata::LevelFilter};
use tracing_subscriber::{
    field::RecordFields,
    fmt::{self, FormatEvent, FormatFields, time::FormatTime},
    registry::LookupSpan,
};

pub static LAST_HOUR: AtomicU8 = AtomicU8::new(0);
pub static TEST_ENV: AtomicBool = AtomicBool::new(false);

struct FieldVisitor<'writer> {
    writer: fmt::format::Writer<'writer>,
}

impl Visit for FieldVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.record_debug(field, &format_args!("{value}"))
        } else {
            self.record_debug(field, &value)
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            write!(
                self.writer,
                "  {} ",
                apply_style_tags(format!("{value:?}"))
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

pub struct FieldFormatter;

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

pub struct EventFormatter {
    pub show_spans: bool,
}

impl FormatTime for EventFormatter {
    fn format_time(&self, writer: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        // if TEST_ENV.load(Ordering::Relaxed) {
        //     return write!(writer, "YYYY-MM-DD");
        // }

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
