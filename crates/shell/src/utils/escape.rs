use std::fmt::Write;

#[derive(Debug, PartialEq)]
enum EscapingMode {
    ANSIHex,
    ANSIBackslash,
    ANSIBacktick,
    Literal,
    Quoted,
}

fn classify(c: char, shell: &str) -> EscapingMode {
    match (shell, c) {
        ("bash", '\u{0}'..='\u{6}') => EscapingMode::ANSIHex,
        ("bash", '\u{7}') => EscapingMode::ANSIHex,
        ("bash", '\u{8}') => EscapingMode::ANSIHex,
        ("bash", '\u{9}') => EscapingMode::ANSIBackslash,
        ("bash", '\u{a}') => EscapingMode::ANSIBackslash,
        ("bash", '\u{b}') => EscapingMode::ANSIHex,
        ("bash", '\u{c}') => EscapingMode::ANSIHex,
        ("bash", '\u{d}') => EscapingMode::ANSIBackslash,
        ("bash", '\u{e}'..='\u{1f}') => EscapingMode::ANSIHex,
        ("bash", ' ' | '!' | '#' | '$' | '%' | '&' | '(' | ')' | '*' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '`' | '{' | '|' | '}' | '~') => EscapingMode::Quoted,
        ("bash", '\'') => EscapingMode::ANSIBackslash,
        ("bash", ':' | '\"' | ',' | '.') => EscapingMode::Quoted,
        ("bash", '\u{30}'..='\u{39}') => EscapingMode::Literal,
        ("bash", '\u{41}'..='\u{5a}') => EscapingMode::Literal,
        ("bash", '_') => EscapingMode::Literal,
        ("bash", '\u{61}'..='\u{7a}') => EscapingMode::Literal,
        ("bash", '\u{7f}') => EscapingMode::ANSIHex,

        ("powershell", '\u{0}'..='\u{6}') => EscapingMode::ANSIHex,
        ("powershell", '\u{9}') => EscapingMode::ANSIBacktick,
        ("powershell", '\u{a}') => EscapingMode::ANSIBacktick,
        ("powershell", '\u{d}') => EscapingMode::ANSIBacktick,
        ("powershell", '\'') => EscapingMode::Quoted,
        ("powershell", _) if c.is_alphanumeric() || c == '_' => EscapingMode::Literal,
        ("powershell", ' ' | '&' | '@' | '*' | '+' | '[' | ']' | '`' | '~') => EscapingMode::Quoted,
        ("powershell", _) => EscapingMode::Quoted,

        ("fish", '\u{0}'..='\u{6}') => EscapingMode::ANSIHex,
        ("fish", '\u{9}') => EscapingMode::ANSIBackslash,
        ("fish", '\u{a}') => EscapingMode::ANSIBackslash,
        ("fish", '\u{d}') => EscapingMode::ANSIBackslash,
        ("fish", '\u{1f}') => EscapingMode::ANSIHex,
        ("fish", '\'') => EscapingMode::ANSIBackslash,
        ("fish", '\\') => EscapingMode::ANSIBackslash,
        ("fish", ' ' | '$' | '&' | '@' | '*' | '+' | '[' | ']' | '`' | '~') => EscapingMode::Quoted,
        ("fish", _) if c.is_ascii_graphic() => EscapingMode::Literal,
        ("fish", _) => EscapingMode::ANSIHex,

        (_, _) => EscapingMode::ANSIHex,
    }
}

fn hexify(c: char, shell: &str) -> String {
    match shell {
        "fish" => format!("'\\X{:02x}'", c as u32),
        "bash" => format!("\\x{:02X}", c as u32),
        "powershell" => format!("`x{:02x}", c as u32),
        _ => format!("`x{:02x}", c as u32),
    }
}

fn backslashify(c: char, shell: &str) -> String {
    if shell == "powershell" {
        format!("`{}", c)
    } else {
        format!("\\{}", c)
    }
}

fn backtickify(c: char) -> String {
    format!("`{}", c)
}

pub fn escape(input: &str, shell: &str) -> String {
    let mut output = String::new();
    let mut escape_needed = false;

    for c in input.chars() {
        match classify(c, shell) {
            EscapingMode::ANSIHex => {
                write!(output, "{}", hexify(c, shell)).unwrap();
                escape_needed = true;
            }
            EscapingMode::ANSIBackslash => {
                write!(output, "{}", backslashify(c, shell)).unwrap();
                escape_needed = true;
            }
            EscapingMode::ANSIBacktick => {
                write!(output, "{}", backtickify(c)).unwrap();
                escape_needed = true;
            }
            EscapingMode::Quoted => {
                if shell == "powershell" && c == '\'' {
                    write!(output, "''").unwrap();
                } else {
                    write!(output, "{}", c).unwrap();
                }
                escape_needed = true;
            }
            EscapingMode::Literal => {
                write!(output, "{}", c).unwrap();
            }
        }
    }

    if escape_needed {
        match shell {
            "bash" => format!("$'{}'", output),
            "powershell" => format!("'{}'", output),
            "fish" => format!("'{}'", output),
            _ => output,
        }
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use starbase_sandbox::assert_snapshot;

    #[test]
    fn test_escape_plain_string() {
        assert_eq!(escape("foobar", "bash"), "foobar");
        assert_eq!(escape("foobar", "powershell"), "foobar");
        assert_eq!(escape("foobar", "fish"), "foobar");
    }

    #[test]
    fn test_escape_string_with_single_space() {
        assert_eq!(escape("foo bar", "bash"), "$'foo bar'");
        assert_eq!(escape("foo bar", "powershell"), "'foo bar'");
        assert_eq!(escape("foo bar", "fish"), "'foo bar'");
    }

    #[test]
    fn test_escape_string_with_multiple_spaces() {
        assert_eq!(escape("foo  bar", "bash"), "$'foo  bar'");
        assert_eq!(escape("foo  bar", "powershell"), "'foo  bar'");
        assert_eq!(escape("foo  bar", "fish"), "'foo  bar'");

        assert_eq!(escape(" foo bar ", "bash"), "$' foo bar '");
        assert_eq!(escape(" foo bar ", "powershell"), "' foo bar '");
        assert_eq!(escape(" foo bar ", "fish"), "' foo bar '");
    }

    #[test]
    fn test_escape_string_with_single_quotes() {
        assert_eq!(escape("don't", "bash"), "$'don\\'t'");
        assert_eq!(escape("don't", "powershell"), "'don''t'");
        assert_eq!(escape("don't", "fish"), "'don\\'t'");
    }

    #[test]
    fn test_escape_string_with_backslashes() {
        assert_eq!(escape("a\\b", "bash"), "$'a\\b'");
        assert_eq!(escape("a\\b", "powershell"), "'a\\b'");
        assert_eq!(escape("a\\b", "fish"), "'a\\\\b'");
    }

    #[test]
    fn test_escape_empty_string() {
        assert_eq!(escape("", "bash"), "");
        assert_eq!(escape("", "powershell"), "");
        assert_eq!(escape("", "fish"), "");
    }

    #[test]
    fn test_escape_string_with_double_quotes() {
        assert_eq!(escape("hello \"world\"", "bash"), "$'hello \"world\"'");
        assert_eq!(escape("hello \"world\"", "powershell"), "'hello \"world\"'");
        assert_eq!(escape("hello \"world\"", "fish"), "'hello \"world\"'");
    }

    #[test]
    fn test_escape_string_with_newline() {
        assert_snapshot!(escape("line1\nline2", "bash"));
        assert_snapshot!(escape("line1\nline2", "powershell"));
        assert_snapshot!(escape("line1\nline2", "fish"));
    }

    #[test]
    fn test_escape_string_with_tab() {
        assert_snapshot!(escape("column1\tcolumn2", "bash"));
        assert_snapshot!(escape("column1\tcolumn2", "powershell"));
        assert_snapshot!(escape("column1\tcolumn2", "fish"));
    }

    #[test]
    fn test_escape_string_with_unicode() {
        assert_eq!(escape("hello😊world", "bash"), "$'hello\\x1F60Aworld'");
        assert_eq!(escape("hello😊world", "powershell"), "'hello😊world'");
        assert_eq!(escape("hello😊world", "fish"), "'hello'\\X1f60a'world'");
    }

    #[test]
    fn test_escape_string_with_all_special_characters() {
        let special_chars = "!@#$%^&*()_+-={}[]:\";'<>?,./\\|`~";
        assert_snapshot!(escape(special_chars, "bash"));
        assert_snapshot!(escape(special_chars, "powershell"));
        assert_snapshot!(escape(special_chars, "fish"));
    }

    #[test]
    fn test_escape_string_with_mixed_characters() {
        let mixed_chars = "Hello, World! 123 $%^&*()";
        assert_eq!(escape(mixed_chars, "bash"), "$'Hello, World! 123 $%^&*()'");
        assert_eq!(escape(mixed_chars, "powershell"), "'Hello, World! 123 $%^&*()'");
        assert_eq!(escape(mixed_chars, "fish"), "'Hello, World! 123 $%^&*()'");
    }

    #[test]
    fn test_escape_string_with_leading_and_trailing_spaces() {
        assert_eq!(escape("  leading and trailing  ", "bash"), "$'  leading and trailing  '");
        assert_eq!(escape("  leading and trailing  ", "powershell"), "'  leading and trailing  '");
        assert_eq!(escape("  leading and trailing  ", "fish"), "'  leading and trailing  '");
    }

    #[test]
    fn test_escape_string_with_special_and_whitespace() {
        let special_whitespace = " $ \t \n @ !";
        assert_snapshot!(escape(special_whitespace, "bash"));
        assert_snapshot!(escape(special_whitespace, "powershell"));
        assert_snapshot!(escape(special_whitespace, "fish"));
    }
}
