use crate::{shell_error::ShellError, shells::*};
use std::path::Path;
use std::str::FromStr;
use std::{env, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellType {
    Bash,
    Elvish,
    Fish,
    Ion,
    Nu,
    Pwsh,
    Zsh,
}

impl ShellType {
    /// Return a list of all shell types.
    pub fn variants() -> Vec<Self> {
        vec![
            Self::Bash,
            Self::Elvish,
            Self::Fish,
            Self::Ion,
            Self::Nu,
            Self::Pwsh,
            Self::Zsh,
        ]
    }

    /// Return a list of shell types for the current operating system
    pub fn os_variants() -> Vec<Self> {
        #[cfg(windows)]
        {
            vec![Self::Bash, Self::Elvish, Self::Fish, Self::Nu, Self::Pwsh]
        }

        #[cfg(not(windows))]
        Self::variants()
    }

    pub fn detect() -> Option<Self> {
        Self::try_detect().ok()
    }

    pub fn try_detect() -> Result<Self, ShellError> {
        if let Some(env_shell) = env::var_os("SHELL") {
            // Don't error if nothing found, continue to the more
            // advanced detection instead
            if let Some(shell) = parse_shell_from_path(env_shell) {
                return Ok(shell);
            }
        }

        Err(ShellError::UnknownShell {
            name: "unknown".into(),
        })
    }

    /// Build a [`Shell`] instance from the current type.
    pub fn build(&self) -> BoxedShell {
        match self {
            Self::Bash => Box::new(Bash),
            Self::Elvish => Box::new(Elvish),
            Self::Fish => Box::new(Fish),
            Self::Ion => Box::new(Ion),
            Self::Nu => Box::new(Nu),
            Self::Pwsh => Box::new(Pwsh),
            Self::Zsh => Box::new(Zsh::new()),
        }
    }
}

impl fmt::Display for ShellType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Bash => "bash",
                Self::Elvish => "elvish",
                Self::Fish => "fish",
                Self::Ion => "ion",
                Self::Nu => "nu",
                Self::Pwsh => "pwsh",
                Self::Zsh => "zsh",
            }
        )
    }
}

impl FromStr for ShellType {
    type Err = ShellError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bash" => Ok(ShellType::Bash),
            "elv" | "elvish" => Ok(ShellType::Elvish),
            "fish" => Ok(ShellType::Fish),
            "ion" => Ok(ShellType::Ion),
            "nu" | "nushell" => Ok(ShellType::Nu),
            "pwsh" | "powershell" | "powershell_ise" => Ok(ShellType::Pwsh),
            "zsh" => Ok(ShellType::Zsh),
            _ => Err(ShellError::UnknownShell {
                name: value.to_owned(),
            }),
        }
    }
}

impl TryFrom<&str> for ShellType {
    type Error = ShellError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<String> for ShellType {
    type Error = ShellError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

fn parse_shell_from_path<P: AsRef<Path>>(path: P) -> Option<ShellType> {
    ShellType::from_str(path.as_ref().file_stem()?.to_str()?).ok()
}
