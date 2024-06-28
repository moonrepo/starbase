use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum ShellError {
    #[cfg_attr(feature = "miette", diagnostic(code(shell::undetected)))]
    #[error("Could not detect your terminal shell. Scanned the $SHELL environment variable and parent processes.")]
    CouldNotDetectShell,

    #[cfg_attr(feature = "miette", diagnostic(code(shell::no_hook_support)))]
    #[error("Your shell, {name}, does not support \"{info}\" hooks. Please remove the command from your shell that is triggering this hook.")]
    NoHookSupport { name: String, info: String },

    #[cfg_attr(feature = "miette", diagnostic(code(shell::unknown)))]
    #[error("Unknown or unsupported shell {name}.")]
    UnknownShell { name: String },
}
