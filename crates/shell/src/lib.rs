mod helpers;
mod hooks;
mod quoter;
mod shell;
mod shell_error;
mod shells;

pub use hooks::*;
pub use quoter::*;
pub use shell::ShellType;
pub use shell_error::ShellError;
pub use shells::*;

use std::ffi::{OsStr, OsString};

/// Join a list of arguments into a command line string using
/// the provided [`Shell`] instance as the quoting mechanism.
pub fn join_args<'a, I, V>(shell: &BoxedShell, args: I) -> String
where
    I: IntoIterator<Item = V>,
    V: Into<Quotable<'a>>,
{
    let mut out = String::new();
    let args = args.into_iter().collect::<Vec<_>>();
    let last_index = args.len() - 1;

    for (index, arg) in args.into_iter().enumerate() {
        let quoted_arg = shell.create_quoter(arg.into()).maybe_quote();

        out.push_str(&quoted_arg);

        if index != last_index {
            out.push(' ');
        }
    }

    out
}

/// Join a list of arguments into a command line string using
/// the provided [`Shell`] instance as the quoting mechanism.
pub fn join_args_os<'a, I, V>(shell: &BoxedShell, args: I) -> OsString
where
    I: IntoIterator<Item = V>,
    V: Into<Quotable<'a>>,
{
    let mut out = OsString::new();
    let args = args.into_iter().collect::<Vec<_>>();
    let last_index = args.len() - 1;

    for (index, arg) in args.into_iter().enumerate() {
        out.push(shell.create_quoter(arg.into()).maybe_quote());

        if index != last_index {
            out.push(OsStr::new(" "));
        }
    }

    out
}
