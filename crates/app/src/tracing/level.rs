use miette::miette;
use std::fmt;
use std::str::FromStr;

// This is similar to tracing `Level` but provides an "Off" variant.
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

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            match self {
                Self::Off => "off",
                Self::Error => "error",
                Self::Warn => "warn",
                Self::Info => "info",
                Self::Debug => "debug",
                Self::Trace => "trace",
            }
        )
    }
}

impl TryFrom<String> for LogLevel {
    type Error = miette::Report;

    fn try_from(value: String) -> Result<Self, <LogLevel as TryFrom<String>>::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<&str> for LogLevel {
    type Error = miette::Report;

    fn try_from(value: &str) -> Result<Self, <LogLevel as TryFrom<&str>>::Error> {
        Self::from_str(value)
    }
}

impl FromStr for LogLevel {
    type Err = miette::Report;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value.to_lowercase().as_str() {
            "off" => Self::Off,
            "error" => Self::Error,
            "warn" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            "trace" => Self::Trace,
            other => return Err(miette!("Unknown log level {other}")),
        })
    }
}
