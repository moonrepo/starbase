use shell_quote::Quotable;
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[cfg(unix)]
pub static PATH_DELIMITER: &str = ":";

#[cfg(windows)]
pub static PATH_DELIMITER: &str = ";";

/// Replace every `${{ key }}` placeholder in the template with its value.
/// Placeholders are the only syntax, so everything else is copied verbatim,
/// braces and all. A placeholder with no value, or one that is never closed,
/// is left in place, where the rendered output makes it obvious.
pub fn render_template(template: &str, values: &[(&str, &str)]) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(index) = rest.find("${{") {
        output.push_str(&rest[..index]);
        rest = &rest[index + 3..];

        let Some(end) = rest.find("}}") else {
            output.push_str("${{");
            break;
        };

        match values.iter().find(|(name, _)| *name == rest[..end].trim()) {
            Some((_, value)) => output.push_str(value),
            None => {
                output.push_str("${{");
                output.push_str(&rest[..end]);
                output.push_str("}}");
            }
        };

        rest = &rest[end + 2..];
    }

    output.push_str(rest);
    output
}

/// Indent every line of a function body, so that a definition reads the way
/// the shell's own code does. Python needs this to parse at all.
pub fn indent_lines(body: &str, prefix: &str) -> String {
    body.lines()
        .map(|line| {
            if line.trim().is_empty() {
                line.to_owned()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn is_absolute_dir(value: OsString) -> Option<PathBuf> {
    let dir = PathBuf::from(&value);

    if !value.is_empty() && dir.is_absolute() {
        Some(dir)
    } else {
        None
    }
}

pub fn get_config_dir(home_dir: &Path) -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .and_then(is_absolute_dir)
        .unwrap_or_else(|| home_dir.join(".config"))
}

pub fn get_var_regex() -> &'static regex::Regex {
    static REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\$(?<name>[A-Za-z0-9_]+)").unwrap());

    &REGEX
}

pub fn get_var_regex_bytes() -> &'static regex::bytes::Regex {
    static REGEX: LazyLock<regex::bytes::Regex> =
        LazyLock::new(|| regex::bytes::Regex::new(r"\$(?<name>[A-Za-z0-9_]+)").unwrap());

    &REGEX
}

pub fn get_env_var_regex() -> &'static regex::Regex {
    static REGEX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\$(?<name>[A-Z0-9_]+)").unwrap());

    &REGEX
}

pub fn get_env_key_native(key: &str) -> &str {
    let is_windows = std::env::consts::OS == "windows";

    if key == "PATH" {
        if is_windows { "Path" } else { "PATH" }
    } else if key == "HOME" {
        if is_windows { "USERPROFILE" } else { "HOME" }
    } else {
        key
    }
}

pub fn normalize_newlines(content: impl AsRef<str>) -> String {
    let content = content.as_ref().trim();

    #[cfg(windows)]
    {
        content.replace('\r', "").replace('\n', "\r\n")
    }

    #[cfg(unix)]
    {
        content.replace('\r', "")
    }
}

#[derive(Debug, Default)]
pub struct ProfileSet {
    pub items: HashMap<PathBuf, u8>,
}

impl ProfileSet {
    pub fn insert(mut self, path: PathBuf, order: u8) -> Self {
        self.items.insert(path, order);

        Self { items: self.items }
    }

    pub fn into_list(self) -> Vec<PathBuf> {
        let mut items = self.items.into_iter().collect::<Vec<_>>();
        items.sort_by_key(|a| a.1);
        items.into_iter().map(|item| item.0).collect()
    }
}

pub fn quotable_into_string(data: Quotable<'_>) -> String {
    match data {
        Quotable::Bytes(bytes) => String::from_utf8_lossy(bytes).into(),
        Quotable::Text(text) => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholders() {
        assert_eq!(
            render_template(
                "fn ${{ name }}() { echo ${{ value }}; }",
                &[("name", "hook"), ("value", "123")]
            ),
            "fn hook() { echo 123; }"
        );
    }

    #[test]
    fn renders_repeated_and_unspaced_placeholders() {
        assert_eq!(
            render_template("${{name}} ${{  name  }} $${{ name }}", &[("name", "hook")]),
            "hook hook $hook"
        );
    }

    #[test]
    fn leaves_shell_syntax_alone() {
        let template = "if [ -n \"${PROMPT:-}\" ]; then f() { echo {}; }; fi";

        assert_eq!(render_template(template, &[("name", "hook")]), template);
    }

    #[test]
    fn keeps_unresolved_placeholders_visible() {
        assert_eq!(
            render_template(
                "${{ known }} ${{ missing }} ${{ unclosed",
                &[("known", "1")]
            ),
            "1 ${{ missing }} ${{ unclosed"
        );
    }
}
