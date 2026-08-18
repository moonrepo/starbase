use starbase_shell::ShellType;
use starbase_styles::color;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Find the absolute path to a command on `PATH`. `pwsh` and `powershell`
/// are treated as aliases of each other, falling back from the former to
/// the latter.
pub fn find_command_on_path(name: &str) -> Option<PathBuf> {
    if name == "pwsh" || name == "powershell" {
        system_env::find_command_on_path("pwsh")
            .or_else(|| system_env::find_command_on_path("powershell"))
    } else {
        system_env::find_command_on_path(name)
    }
}

/// Detect the current shell, falling back to a sensible default. The
/// result is cached after the first call.
pub fn get_default_shell() -> ShellType {
    static SHELL_CACHE: OnceLock<ShellType> = OnceLock::new();

    *SHELL_CACHE.get_or_init(ShellType::detect_with_fallback)
}

/// Format a command line for display, styled and suffixed with the
/// directory it ran in, relative to the workspace root.
pub fn format_command_line(command: &str, root_dir: &Path, working_dir: &Path) -> String {
    let dir = if working_dir == root_dir {
        "workspace".into()
    } else if let Ok(dir) = working_dir.strip_prefix(root_dir) {
        format!(".{}{}", std::path::MAIN_SEPARATOR, dir.to_string_lossy())
    } else {
        ".".into()
    };

    format!(
        "{} {}",
        color::muted_light(command.trim()),
        color::muted(format!("(in {dir})"))
    )
}
