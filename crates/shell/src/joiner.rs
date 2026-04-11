use crate::shells::BoxedShell;
use shell_quote::Quotable;
use std::ffi::{OsStr, OsString};

#[cfg(unix)]
fn into_quotable(arg: &OsStr) -> Quotable<'_> {
    Quotable::from(arg)
}

// Windows does not support `OsStr` or `OsString`.
// https://github.com/allenap/shell-quote/issues/39
#[cfg(windows)]
fn into_quotable(arg: &OsStr) -> Quotable<'_> {
    Quotable::from(arg.as_encoded_bytes())
}

fn apply_quote(
    shell: &BoxedShell,
    value: &OsStr,
    force_quote: bool,
    check_path: bool,
    out: &mut OsString,
) {
    let apply = force_quote
        || value
            .as_encoded_bytes()
            .iter()
            .any(|b| b == &b' ' || (check_path && b == &b'/') || (check_path && b == &b'\\'));

    if apply {
        out.push(shell.quote_with(into_quotable(value)));
    } else {
        out.push(value);
    }
}

/// Join an executable and its arguments into a single string. This function *will not* auto-quote
/// all arguments by default, and will only quote when an argument contains spaces or slashes.
pub fn join_exe_args<I, A>(shell: &BoxedShell, exe: A, args: I, force_quote: bool) -> OsString
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let mut list = vec![exe];
    list.extend(args);

    join_args(shell, list, force_quote)
}

/// Join a list of arguments into a single string. This function *will not* auto-quote
/// all arguments by default, and will only quote when an argument contains spaces or slashes.
pub fn join_args<I, A>(shell: &BoxedShell, args: I, force_quote: bool) -> OsString
where
    I: IntoIterator<Item = A>,
    A: AsRef<OsStr>,
{
    let mut out = OsString::new();

    for (i, arg) in args.into_iter().enumerate() {
        let arg = arg.as_ref();

        if i > 0 {
            out.push(OsStr::new(" "));
        }

        apply_quote(shell, arg, force_quote, i == 0, &mut out);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ShellType;

    #[test]
    fn test_join_exe_args() {
        let shell = ShellType::Bash.build();
        let result = join_exe_args(&shell, "echo", ["hello world", "foo"], false);

        assert_eq!(result, "echo $'hello world' foo");

        let result = join_exe_args(&shell, "echo", ["hello world", "foo"], true);

        assert_eq!(result, "echo $'hello world' foo");
    }

    #[test]
    fn quotes_spaces_in_exe() {
        let shell = ShellType::Bash.build();
        let result = join_exe_args(&shell, "some file", ["hello world"], false);

        assert_eq!(result, "$'some file' $'hello world'");

        let shell = ShellType::Pwsh.build();
        let result = join_exe_args(&shell, "some file", ["hello world"], false);

        assert_eq!(result, "'some file' 'hello world'");
    }

    #[test]
    fn quotes_slashes_in_exe() {
        let shell = ShellType::Bash.build();
        let result = join_exe_args(&shell, "some/file", ["hello world"], false);

        assert_eq!(result, "some/file $'hello world'");

        let shell = ShellType::Pwsh.build();
        let result = join_exe_args(&shell, "some\\file", ["hello world"], false);

        assert_eq!(result, "\"some\\\\file\" 'hello world'");
    }

    #[test]
    fn pwsh_patterns() {
        let shell = ShellType::Pwsh.build();
        let result = join_exe_args(&shell, "Write-Output", ["hello world"], false);

        assert_eq!(result, "Write-Output 'hello world'");

        let result = join_exe_args(&shell, "Get-Content", ["some\\file\\path.txt"], false);

        assert_eq!(result, "Get-Content some\\file\\path.txt");
    }
}
