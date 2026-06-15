use miette::miette;
use std::fmt;
use std::str::FromStr;

/// This is similar to tracing `Level` but provides "Off" and "Verbose" variants.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
    /// Like tracing, but also includes all modules and spans.
    Verbose,
}

impl LogLevel {
    pub fn is_off(&self) -> bool {
        matches!(self, Self::Off)
    }

    pub fn is_verbose(&self) -> bool {
        matches!(self, Self::Verbose)
    }

    #[cfg(feature = "tracing")]
    pub fn to_tracing_level(&self) -> Option<tracing::Level> {
        use tracing::Level;

        match self {
            Self::Off => None,
            Self::Error => Some(Level::ERROR),
            Self::Warn => Some(Level::WARN),
            Self::Info => Some(Level::INFO),
            Self::Debug => Some(Level::DEBUG),
            Self::Trace | Self::Verbose => Some(Level::TRACE),
        }
    }
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
                Self::Verbose => "verbose",
            }
        )
    }
}

impl TryFrom<String> for LogLevel {
    type Error = miette::Report;

    fn try_from(value: String) -> Result<Self, miette::Report> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<&str> for LogLevel {
    type Error = miette::Report;

    fn try_from(value: &str) -> Result<Self, miette::Report> {
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
            "verbose" => Self::Verbose,
            other => return Err(miette!("Unknown log level {other}")),
        })
    }
}
