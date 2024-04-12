use std::path::Path;

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
    /// Return a list of all shell variants.
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
}

fn parse_shell_from_path(path: &Path) -> Option<ShellType> {
    match path.file_stem()?.to_str()? {
        "bash" => Some(ShellType::Bash),
        "elvish" => Some(ShellType::Elvish),
        "fish" => Some(ShellType::Fish),
        "ion" => Some(ShellType::Ion),
        "nu" | "nushell" => Some(ShellType::Nu),
        "powershell" | "powershell_ise" | "pwsh" => Some(ShellType::Pwsh),
        "zsh" => Some(ShellType::Zsh),
        _ => None,
    }
}
