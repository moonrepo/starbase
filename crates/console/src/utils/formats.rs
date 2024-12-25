use std::time::Duration;

pub fn format_float(value: f64) -> String {
    format!("{value:.1}").replace(".0", "")
}

pub const DECIMAL_BYTE_UNITS: &[&str] = &["B", "kB", "MB", "GB", "TB", "PB"];
pub const BINARY_BYTE_UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

fn format_bytes(mut size: f64, kb: f64, units: &[&str]) -> String {
    if size < kb {
        return format!("{size}{}", units[0]);
    }

    let mut prefix = 1;

    while size >= kb && prefix < 6 {
        size /= kb;
        prefix += 1;
    }

    format!("{} {}", format_float(size), units[prefix - 1])
}

pub fn format_bytes_binary(size: u64) -> String {
    format_bytes(size as f64, 1024.0, BINARY_BYTE_UNITS)
}

pub fn format_bytes_decimal(size: u64) -> String {
    format_bytes(size as f64, 1000.0, DECIMAL_BYTE_UNITS)
}

pub const NANOSECOND: Duration = Duration::from_nanos(1_000_000_000);
pub const MICROSECOND: Duration = Duration::from_micros(1_000_000);
pub const MILLISECOND: Duration = Duration::from_millis(1_000);
pub const SECOND: Duration = Duration::from_secs(1);
pub const MINUTE: Duration = Duration::from_secs(60);
pub const HOUR: Duration = Duration::from_secs(60 * 60);
pub const DAY: Duration = Duration::from_secs(24 * 60 * 60);
pub const WEEK: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const YEAR: Duration = Duration::from_secs(365 * 24 * 60 * 60);

pub const DURATION_UNITS: &[(Duration, &str, &str, &str)] = &[
    (NANOSECOND, "nanosecond", "nanoseconds", "ns"),
    (MICROSECOND, "microsecond", "microseconds", "μs"),
    (MILLISECOND, "millisecond", "milliseconds", "ms"),
    (SECOND, "second", "seconds", "s"),
    (MINUTE, "minute", "minutes", "m"),
    (HOUR, "hour", "hours", "h"),
    (DAY, "day", "days", "d"),
    (WEEK, "week", "weeks", "w"),
    (YEAR, "year", "years", "y"),
];

pub fn format_duration(duration: Duration, short_suffix: bool) -> String {
    let mut nanos = duration.as_nanos();
    let mut output: Vec<String> = vec![];

    for (d, long, long_plural, short) in DURATION_UNITS.iter().rev() {
        if nanos == 0 {
            break;
        }

        let mut count = 0;
        let amount = d.as_nanos();

        while nanos > amount {
            nanos -= amount;
            count += 1;
        }

        if count > 0 {
            output.push(if short_suffix {
                format!("{count}{short}")
            } else if count == 1 {
                format!("{count} {long}")
            } else {
                format!("{count} {long_plural}")
            });
        }
    }

    if output.is_empty() {
        return "0s".into();
    }

    output.join(" ")
}
