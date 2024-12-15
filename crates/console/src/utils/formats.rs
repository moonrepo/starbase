use std::time::Duration;

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

    format!("{size:.1} {}", units[prefix - 1]).replace(".0", "")
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

pub const DURATION_UNITS: &[(Duration, &str, &str)] = &[
    (NANOSECOND, "nanosecond", "ns"),
    (MICROSECOND, "microsecond", "μs"),
    (MILLISECOND, "millisecond", "ms"),
    (SECOND, "second", "s"),
    (MINUTE, "minute", "m"),
    (HOUR, "hour", "h"),
    (DAY, "day", "d"),
    (WEEK, "week", "w"),
    (YEAR, "year", "y"),
];

pub fn format_duration(duration: Duration, short_suffix: bool) -> String {
    let mut nanos = duration.as_nanos();
    let mut output: Vec<String> = vec![];

    for (d, long, short) in DURATION_UNITS.iter().rev() {
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
            output.push(format!(
                "{count}{}",
                if short_suffix { short } else { long }
            ));
        }
    }

    if output.is_empty() {
        return "0s".into();
    }

    output.join(" ")
}
