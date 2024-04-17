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
        let env_shell = env::var_os("SHELL");

        if let Some(env_value) = &env_shell {
            // Don't error if nothing found, continue to the more
            // advanced OS-specific detection instead
            if let Some(shell) = parse_shell_from_path(env_value) {
                return Ok(shell);
            }
        }

        if let Some(shell) = detect_from_env() {
            return Ok(shell);
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

pub fn parse_shell_from_path<P: AsRef<Path>>(path: P) -> Option<ShellType> {
    ShellType::from_str(path.as_ref().file_stem()?.to_str()?).ok()
}

fn detect_from_env() -> Option<ShellType> {
    #[cfg(not(windows))]
    {
        unix::detect()
    }

    #[cfg(windows)]
    None
}

#[cfg(not(windows))]
mod unix {
    use super::*;
    use std::io::BufRead;
    use std::process::{self, Command};

    pub struct ProcessStatus {
        ppid: Option<u32>,
        comm: String,
    }

    // PPID COMM
    //  635 -zsh
    pub fn detect_from_process_status(current_pid: u32) -> Option<ProcessStatus> {
        let output = Command::new("ps")
            .args(["-o", "ppid,comm"])
            .arg(current_pid.to_string())
            .output()
            .ok()?;

        let mut lines = output.stdout.lines();
        let line = lines.nth(1)?.ok()?;
        let mut parts = line.trim().split_whitespace();

        match (parts.next(), parts.next()) {
            (Some(ppid), Some(comm)) => Some(ProcessStatus {
                ppid: ppid.parse().ok(),
                comm: comm.strip_prefix("-").unwrap_or(comm).to_owned(),
            }),
            _ => None,
        }
    }

    pub fn detect() -> Option<ShellType> {
        let mut pid = Some(process::id());
        let mut depth = 0;

        while let Some(current_pid) = pid {
            if depth > 10 {
                return None;
            }

            let status = detect_from_process_status(current_pid)?;

            if let Some(shell) = parse_shell_from_path(status.comm) {
                return Some(shell);
            }

            pid = status.ppid;
            depth += 1;
        }

        None
    }
}
