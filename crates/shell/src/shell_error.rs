use starbase_styles::{Style, Stylize};
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum ShellError {
    #[cfg_attr(feature = "miette", diagnostic(code(shell::unknown)))]
    #[error("Unknown or unsupported shell {}.", .name.style(Style::Id))]
    UnknownShell { name: String },
}
