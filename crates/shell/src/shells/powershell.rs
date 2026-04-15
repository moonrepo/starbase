use super::Shell;
use crate::helpers::{ProfileSet, get_env_key_native, get_env_var_regex, normalize_newlines};
use crate::hooks::*;
use crate::quoter::*;
use base64::Engine;
use shell_quote::Quotable;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

// On Unix, arguments are passed to a command using `argv` (a list of strings),
// and do not need to be quoted/escaped because they are never joined into a string.
// On Windows (sigh), arguments are passed to a command using `GetCommandLineW`
// (a single string), and so they need to be quoted/escaped properly to ensure they
// are parsed correctly by the receiving process.
//
// Internally Rust will join all arguments into a string and attempt to quote/escape
// them, which is documented here:
// https://github.com/rust-lang/rust/blob/main/library/std/src/sys/process/windows.rs#L859
//
// The quoting/escaping rules for each argument is documented here:
// https://github.com/rust-lang/rust/blob/main/library/std/src/sys/args/windows.rs#L173
//
// Let's demonstrate this. If we have the executable `echo` and the arguments
// `["hello world", "foo"]`, Rust will join them into this string:
//
//     echo "hello world" foo
//
// The last argument is not quoted because it does not contain any spaces, but the first
// argument is quoted because it contains a space. This makes sense when the command is
// NOT run in a shell, because when we do run in a shell, we need to join the command line
// into a single string ourselves, and then pass it as an argument to the shell.
//
// For example, using the same command above, if we want to run it in PowerShell, we need
// to join it and the executable is now `pwsh` and the arguments are now
// `["-c", "echo \"hello world\" foo"]`. This makes sense, but now we're in another
// layer of quoting/escaping, which can be difficult to get right.
//
// Internally Rust will join them into this string:
//
//     pwsh -c "echo \"hello world\" foo"
//
// But what happens when we have multiple nested quotes, each with their own escaping, and
// then Rust applies even more quoting/escaping on top of that? It becomes a nightmare.
// I have spent WAY too much time trying to get this right, and I have come to the conclusion
// that the only way to get this right is to use `-EncodedCommand` in PowerShell.
// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1#-encodedcommand-base64encodedcommand
//
// This will obfuscate the command being ran (if debugging the `Command` directly), but it will
// save so much time and headache trying to get the quoting/escaping right, and it will also be
// more robust to edge cases.

fn base64_encode<T: AsRef<OsStr>>(decoded: T) -> String {
    // The string must be formatted using UTF-16LE character encoding:
    // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_powershell_exe?view=powershell-5.1#-encodedcommand-base64encodedcommand
    let utf16: Vec<u8> = decoded
        .as_ref()
        .to_string_lossy()
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();

    // We should include padding:
    // https://learn.microsoft.com/en-us/dotnet/api/system.convert.tobase64string?view=net-10.0#remarks
    base64::prelude::BASE64_STANDARD.encode(&utf16)
}

#[derive(Clone, Copy, Debug)]
pub struct PowerShell;

impl PowerShell {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self
    }

    // $FOO -> $env:FOO
    fn replace_env(&self, value: impl AsRef<str>) -> String {
        get_env_var_regex()
            .replace_all(value.as_ref(), "$$env:$name")
            // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables?view=powershell-5.1#home
            .replace("$env:HOME", "$HOME")
    }

    fn join_path(&self, value: impl AsRef<str>) -> String {
        let value = value.as_ref();

        // When no variable, return as-is
        if !value.contains('$') {
            return format!("\"{value}\"");
        }

        // Otherwise split into segments and join
        let parts = self
            .replace_env(value)
            .split(['/', '\\'])
            .map(|part| {
                if part.starts_with('$') {
                    part.to_owned()
                } else {
                    format!("\"{part}\"")
                }
            })
            .collect::<Vec<_>>();

        if parts.len() == 1 {
            return parts.join("");
        }

        format!("Join-Path {}", parts.join(" "))
    }
}

// https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-5.1
impl Shell for PowerShell {
    fn create_quoter<'a>(&self, data: Quotable<'a>) -> Quoter<'a> {
        let mut options = QuoterOptions {
            quoted_syntax: vec![
                Syntax::Pair("$(".into(), ")".into()),
                Syntax::Pair("${".into(), "}".into()),
            ],
            ..Default::default()
        };

        // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_quoting_rules
        options.replacements.insert('\'', "''");
        options.replacements_expansion.insert('"', "\"\"");

        Quoter::new(data, options)
    }

    /// Build a PowerShell/pwsh `Command` that runs `script` via `-EncodedCommand`.
    /// `program` is the executable name (`powershell` or `pwsh`).
    ///
    /// The script is encoded as UTF-16LE + base64. PowerShell decodes it
    /// internally, which bypasses the mismatch between Rust's Windows argument
    /// quoting (MSVCRT-style `\"`) and PowerShell's own parser (`""` / backtick).
    fn create_wrapped_command_with(&self, script: OsString) -> Command {
        let mut command = Command::new(self.to_string());
        command.args(["-NoLogo", "-NoProfile", "-EncodedCommand"]);
        command.arg(base64_encode(script));
        command
    }

    fn format(&self, statement: Statement<'_>) -> String {
        match statement {
            Statement::ModifyPath {
                paths,
                key,
                orig_key,
            } => {
                let key = key.unwrap_or("PATH");
                let mut value = format!("$env:{} = @(\n", get_env_key_native(key));

                for path in paths {
                    let path = self.join_path(path);

                    if path.starts_with("Join-Path") {
                        value.push_str(&format!("  ({path})\n"));
                    } else {
                        value.push_str(&format!("  {path}\n"));
                    }
                }

                if let Some(orig_key) = orig_key {
                    value.push_str("  $env:");
                    value.push_str(get_env_key_native(orig_key));
                    value.push('\n');
                }

                value.push_str(") -join [IO.PATH]::PathSeparator;");

                normalize_newlines(value)
            }
            Statement::SetAlias { name, value } => {
                format!(
                    "Set-Alias -Name {} -Value {};",
                    self.quote(name),
                    self.quote(value)
                )
            }
            Statement::SetEnv { key, value } => {
                let key = get_env_key_native(key);

                if value.contains('/') || value.contains('\\') {
                    format!("$env:{} = {};", key, self.join_path(value))
                } else {
                    format!(
                        "$env:{} = {};",
                        key,
                        self.quote(self.replace_env(value).as_str())
                    )
                }
            }
            Statement::UnsetAlias { name } => {
                format!("Remove-Alias -Name {} -Force;", self.quote(name))
            }
            Statement::UnsetEnv { key } => {
                format!(
                    r#"if (Test-Path "env:{}") {{
  Remove-Item -LiteralPath "env:{key}";
}}"#,
                    get_env_key_native(key)
                )
            }
        }
    }

    fn get_config_path(&self, home_dir: &Path) -> PathBuf {
        home_dir
            .join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1")
    }

    fn get_env_path(&self, home_dir: &Path) -> PathBuf {
        self.get_config_path(home_dir)
    }

    fn get_env_regex(&self) -> regex::Regex {
        regex::Regex::new(r"\$(Env|env):(?<name>[A-Za-z0-9_]+)").unwrap()
    }

    fn get_profile_paths(&self, home_dir: &Path) -> Vec<PathBuf> {
        let mut profiles = ProfileSet::default();

        // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_profiles?view=powershell-5.1#the-profile-variable
        if let Some(profile) = env::var_os("PROFILE") {
            profiles = profiles.insert(PathBuf::from(profile), 10);
        }

        let docs_dir = home_dir.join("Documents");

        profiles = profiles
            .insert(docs_dir.join("WindowsPowerShell").join("Profile.ps1"), 1)
            .insert(
                docs_dir
                    .join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
                2,
            );

        profiles.into_list()
    }
}

impl fmt::Display for PowerShell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "powershell")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a base64 string produced by `base64_encode`, then interpret the
    /// bytes as UTF-16LE and return the resulting Rust string. Used only by
    /// the tests below to assert the wrapped command round-trips.
    fn base64_decode<T: AsRef<str>>(encoded: T) -> String {
        let bytes = base64::prelude::BASE64_STANDARD
            .decode(encoded.as_ref())
            .unwrap();

        let utf16: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();

        String::from_utf16(&utf16).unwrap()
    }

    #[test]
    fn creates_wrapped_command() {
        let command = PowerShell.create_wrapped_command_with("echo hello".into());
        assert_eq!(command.get_program(), "powershell");

        let args: Vec<_> = command.get_args().collect();
        assert_eq!(&args[..3], &["-NoLogo", "-NoProfile", "-EncodedCommand"]);
        assert_eq!(base64_decode(args[3].to_str().unwrap()), "echo hello");
    }

    #[test]
    fn creates_wrapped_command_with_quotes() {
        // The `&` call-operator workaround is no longer needed: -EncodedCommand
        // bypasses PowerShell's command-line parser entirely, so a leading
        // quote is just data.
        let command = PowerShell.create_wrapped_command_with("'echo' hello".into());
        assert_eq!(command.get_program(), "powershell");

        let args: Vec<_> = command.get_args().collect();
        assert_eq!(&args[..3], &["-NoLogo", "-NoProfile", "-EncodedCommand"]);
        assert_eq!(base64_decode(args[3].to_str().unwrap()), "'echo' hello");
    }

    #[test]
    fn creates_wrapped_command_with_embedded_double_quotes() {
        // This is the case that was broken: Rust's Windows arg escaping
        // emits `\"` for embedded quotes, which PowerShell mis-parses.
        // With -EncodedCommand the exact bytes round-trip.
        let script = r#"Write-Host "he said ""hi"""; $x = 'a b'; dir C:\temp"#;
        let command = PowerShell.create_wrapped_command_with(script.into());

        let args: Vec<_> = command.get_args().collect();
        assert_eq!(base64_decode(args[3].to_str().unwrap()), script);
    }

    #[test]
    fn base64_encode_known_vectors() {
        // Standard RFC 4648 test vectors.
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("f"), "ZgA=");
        assert_eq!(base64_encode("fo"), "ZgBvAA==");
        assert_eq!(base64_encode("foo"), "ZgBvAG8A");
        assert_eq!(base64_encode("foob"), "ZgBvAG8AYgA=");
        assert_eq!(base64_encode("fooba"), "ZgBvAG8AYgBhAA==");
        assert_eq!(base64_encode("foobar"), "ZgBvAG8AYgBhAHIA");
    }

    #[test]
    fn formats_env_var() {
        assert_eq!(
            PowerShell.format_env_set("PROTO_HOME", "$HOME/.proto"),
            r#"$env:PROTO_HOME = Join-Path $HOME ".proto";"#
        );
        assert_eq!(
            PowerShell.format_env_set("PROTO_HOME", "$HOME"),
            r#"$env:PROTO_HOME = "$HOME";"#
        );
        assert_eq!(
            PowerShell.format_env_set("BOOL", "true"),
            r#"$env:BOOL = true;"#
        );
        assert_eq!(
            PowerShell.format_env_set("STRING", "a b c"),
            r#"$env:STRING = 'a b c';"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            PowerShell
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $HOME
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $env:BINPATH
  "C:\absolute\path"
  $env:PATH
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            PowerShell
                .format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $HOME
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:PATH = @(
  $env:BINPATH
  "C:\absolute\path"
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_prepend() {
        assert_eq!(
            PowerShell
                .format_path_prepend(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $HOME
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_prepend(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $env:BINPATH
  "C:\absolute\path"
  $env:Path
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn formats_path_set() {
        assert_eq!(
            PowerShell
                .format_path_set(&["$PROTO_HOME/shims".into(), "$PROTO_HOME\\bin".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  (Join-Path $env:PROTO_HOME "shims")
  (Join-Path $env:PROTO_HOME "bin")
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$HOME".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $HOME
) -join [IO.PATH]::PathSeparator;"#
        );

        assert_eq!(
            PowerShell
                .format_path_set(&["$BINPATH".into(), "C:\\absolute\\path".into()])
                .replace("\r\n", "\n"),
            r#"$env:Path = @(
  $env:BINPATH
  "C:\absolute\path"
) -join [IO.PATH]::PathSeparator;"#
        );
    }

    #[test]
    fn test_profile_paths() {
        #[allow(deprecated)]
        let home_dir = std::env::home_dir().unwrap();

        assert_eq!(
            PowerShell::new().get_profile_paths(&home_dir),
            vec![
                home_dir
                    .join("Documents")
                    .join("WindowsPowerShell")
                    .join("Profile.ps1"),
                home_dir
                    .join("Documents")
                    .join("WindowsPowerShell")
                    .join("Microsoft.PowerShell_profile.ps1"),
            ]
        );
    }

    #[test]
    fn formats_alias_set() {
        assert_eq!(
            PowerShell.format_alias_set("ll", "Get-ChildItem"),
            "Set-Alias -Name ll -Value 'Get-ChildItem';"
        );
    }

    #[test]
    fn formats_alias_unset() {
        assert_eq!(
            PowerShell.format_alias_unset("ll"),
            "Remove-Alias -Name ll -Force;"
        );
    }

    #[test]
    fn test_pwsh_quoting() {
        assert_eq!(PowerShell.quote(""), "''");
        assert_eq!(PowerShell.quote("simple"), "simple");
        assert_eq!(PowerShell.quote("don't"), "'don''t'");
        assert_eq!(PowerShell.quote("say \"hello\""), "\"say \"\"hello\"\"\"");
        assert_eq!(PowerShell.quote("back`tick"), "'back`tick'");
        // assert_eq!(PowerShell.quote("price $5"), "\"price `$5\"");
    }
}
