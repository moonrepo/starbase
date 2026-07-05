use crate::helpers::{get_var_regex, get_var_regex_bytes, quotable_into_string};
use std::collections::HashMap;
use std::sync::Arc;

pub use shell_quote::Quotable;

pub fn default_escape_chars() -> HashMap<char, &'static str> {
    HashMap::from_iter([
        // Zero byte
        ('\0', "\x00"),
        // Bell
        ('\x07', "\\a"),
        // Backspace
        ('\x08', "\\b"),
        // Horizontal tab
        ('\x09', "\\t"),
        ('\t', "\\t"),
        // Newline
        ('\x0A', "\\n"),
        ('\n', "\\n"),
        // Vertical tab
        ('\x0B', "\\v"),
        // Form feed
        ('\x0C', "\\f"),
        // Carriage return
        ('\x0D', "\\r"),
        ('\r', "\\r"),
        // Escape
        ('\x1B', "\\e"),
        // Double quote
        ('"', "\\\""),
        // Backslash
        ('\\', "\\\\"),
    ])
}

/// Escape characters for the double-quoted (expansion) context of POSIX shells
/// (bash, zsh, sh, dash, ash). Inside double quotes only `"`, `\` and `` ` `` are
/// special, so those are the only characters escaped here. Control characters are
/// deliberately omitted: unlike `$'...'` (ANSI-C) quoting, a double-quoted string
/// does *not* interpret `\n`, `\t`, etc. as escape sequences, so emitting them here
/// would corrupt the value. Such values are instead routed to the `$'...'` quoter.
/// `$` is left unescaped so `$VAR` expansion still works; `` ` `` is escaped to
/// prevent command substitution.
pub fn posix_expansion_escape_chars() -> HashMap<char, &'static str> {
    HashMap::from_iter([
        // Double quote
        ('"', "\\\""),
        // Backslash
        ('\\', "\\\\"),
        // Backtick (command substitution)
        ('`', "\\`"),
    ])
}

pub fn apply_quote(
    value: Quotable<'_>,
    quotes: (&str, &str),
    replacements: &HashMap<char, &str>,
) -> String {
    let value = quotable_into_string(value);
    let (open, close) = quotes;

    let mut out = String::with_capacity(open.len() + value.len() + close.len());
    out.push_str(open);

    for ch in value.chars() {
        if let Some(replacement) = replacements.get(&ch) {
            out.push_str(replacement);
        } else {
            out.push(ch);
        }
    }

    out.push_str(close);
    out
}

/// Types of syntax to check for to determine quoting.
pub enum Syntax {
    Symbol(String),
    Pair(String, String),
}

/// Options for [`Quoter`].
pub struct QuoterOptions<'a> {
    /// List of start and end quotes for strings.
    /// The boolean indicates whether the quotes should be used for expansion or not.
    pub quote_pairs: Vec<(String, String, bool)>,

    /// List of syntax and characters that must be quoted for expansion.
    pub quoted_syntax: Vec<Syntax>,

    /// List of syntax and characters that must not be quoted.
    pub unquoted_syntax: Vec<Syntax>,

    /// Handler to apply quoting for non-expansion, typically for single quotes.
    pub on_quote: Option<Arc<dyn Fn(Quotable<'a>) -> String>>,

    /// Handler to apply quoting for expansion, typically for double quotes.
    pub on_quote_expansion: Option<Arc<dyn Fn(Quotable<'a>) -> String>>,

    /// Map of characters to replace with during non-expansion, typically for escaping.
    pub replacements: HashMap<char, &'a str>,

    /// Map of characters to replace with during expansion, typically for escaping.
    pub replacements_expansion: HashMap<char, &'a str>,
}

impl Default for QuoterOptions<'_> {
    fn default() -> Self {
        Self {
            quote_pairs: vec![
                ("'".into(), "'".into(), false),
                ("\"".into(), "\"".into(), true),
            ],
            // https://www.gnu.org/software/bash/manual/bash.html#Shell-Expansions
            quoted_syntax: vec![
                // param
                Syntax::Pair("${".into(), "}".into()),
                // command
                Syntax::Pair("$(".into(), ")".into()),
                // arithmetic
                Syntax::Pair("$((".into(), "))".into()),
            ],
            unquoted_syntax: vec![
                // brace
                Syntax::Pair("{".into(), "}".into()),
                // process
                Syntax::Pair("<(".into(), ")".into()),
                Syntax::Pair(">(".into(), ")".into()),
                // file, glob
                Syntax::Symbol("**".into()),
                Syntax::Symbol("*".into()),
                Syntax::Symbol("?".into()),
                Syntax::Pair("[".into(), "]".into()),
                Syntax::Pair("?(".into(), ")".into()),
                Syntax::Pair("*(".into(), ")".into()),
                Syntax::Pair("+(".into(), ")".into()),
                Syntax::Pair("@(".into(), ")".into()),
                Syntax::Pair("!(".into(), ")".into()),
            ],
            on_quote: None,
            on_quote_expansion: None,
            replacements: HashMap::default(),
            replacements_expansion: default_escape_chars(),
        }
    }
}

/// A utility for quoting a string.
pub struct Quoter<'a> {
    data: Quotable<'a>,
    options: QuoterOptions<'a>,
}

impl<'a> Quoter<'a> {
    /// Create a new instance.
    pub fn new(data: impl Into<Quotable<'a>>, options: QuoterOptions<'a>) -> Quoter<'a> {
        Self {
            data: data.into(),
            options,
        }
    }

    /// Return true if the provided string is a bareword.
    pub fn is_bareword(&self) -> bool {
        match &self.data {
            // An empty string is not a bareword (it needs explicit `''`).
            Quotable::Bytes(bytes) => {
                !bytes.is_empty()
                    && bytes
                        .iter()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == &b'_')
            }
            // Test each `char` directly. A previous version cast `char as u8`, which
            // truncated non-ASCII code points (e.g. 'Ł' U+0141 -> 0x41 = 'A') and
            // misclassified them as barewords. Any non-ASCII char is not a bareword.
            Quotable::Text(text) => {
                !text.is_empty()
                    && text
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            }
        }
    }

    /// Return true if the provided string is empty.
    pub fn is_empty(&self) -> bool {
        match &self.data {
            Quotable::Bytes(bytes) => bytes.is_empty(),
            Quotable::Text(text) => text.is_empty(),
        }
    }

    /// Return true if the provided string is already quoted.
    pub fn is_quoted(&self) -> bool {
        for (sq, eq, _) in &self.options.quote_pairs {
            // The value must be long enough to hold both the opening and closing
            // quote without them overlapping, otherwise a lone quote character
            // (e.g. `'`) would be mistaken for an already-quoted string and
            // passed through unquoted.
            match &self.data {
                Quotable::Bytes(bytes) => {
                    if bytes.len() >= sq.len() + eq.len()
                        && bytes.starts_with(sq.as_bytes())
                        && bytes.ends_with(eq.as_bytes())
                    {
                        let interior = &bytes[sq.len()..bytes.len() - eq.len()];

                        if !contains_subslice(interior, eq.as_bytes()) {
                            return true;
                        }
                    }
                }
                Quotable::Text(text) => {
                    if text.len() >= sq.len() + eq.len()
                        && text.starts_with(sq)
                        && text.ends_with(eq)
                    {
                        // The interior (between the opening and closing quotes) must
                        // not contain the closing delimiter. Otherwise a value like
                        // `'foo'bar'baz'` would be treated as a single already-quoted
                        // token and passed through unquoted, silently concatenating to
                        // `foobarbaz` (or worse, breaking out of the quoting).
                        let interior = &text[sq.len()..text.len() - eq.len()];

                        if !interior.contains(eq.as_str()) {
                            return true;
                        }
                    }
                }
            };
        }

        false
    }

    /// Maybe quote the provided string depending on certain conditions.
    /// If it's already quoted, do nothing. If it requires expansion,
    /// use shell-specific quotes. Otherwise quote as normal.
    pub fn maybe_quote(self) -> String {
        if self.is_empty() {
            let pair = &self.options.quote_pairs[0];

            return format!("{}{}", pair.0, pair.1);
        }

        if self.is_quoted() || self.is_bareword() {
            return quotable_into_string(self.data);
        }

        if self.requires_expansion() {
            return self.quote_expansion();
        }

        if self.requires_unquoted() {
            return quotable_into_string(self.data);
        }

        self.quote()
    }

    /// Quote the provided string for expansion, substition, etc.
    /// This assumes the string is not already quoted.
    pub fn quote_expansion(self) -> String {
        if let Some(on_quote_expansion) = &self.options.on_quote_expansion {
            return on_quote_expansion(self.data);
        }

        let (open, close, _) = self
            .options
            .quote_pairs
            .iter()
            .find(|(_, _, is_expansion)| *is_expansion)
            .or(self.options.quote_pairs.last())
            .unwrap();

        apply_quote(
            self.data,
            (open, close),
            &self.options.replacements_expansion,
        )
    }

    /// Quote the provided string.
    /// This assumes the string is not already quoted.
    pub fn quote(self) -> String {
        if let Some(on_quote) = &self.options.on_quote {
            return on_quote(self.data);
        }

        let (open, close, _) = self
            .options
            .quote_pairs
            .iter()
            .find(|(_, _, is_expansion)| !is_expansion)
            .or(self.options.quote_pairs.first())
            .unwrap();

        apply_quote(self.data, (open, close), &self.options.replacements)
    }

    /// Return true if the provided string requires expansion.
    pub fn requires_expansion(&self) -> bool {
        // Unique syntax
        if quotable_contains_syntax(&self.data, &self.options.quoted_syntax) {
            return true;
        }

        // Variables
        if match &self.data {
            Quotable::Bytes(bytes) => get_var_regex_bytes().is_match(bytes),
            Quotable::Text(text) => get_var_regex().is_match(text),
        } {
            return true;
        }

        // Replacements / escape chars
        for ch in self.options.replacements_expansion.keys() {
            match &self.data {
                Quotable::Bytes(bytes) => {
                    if bytes.contains(&(*ch as u8)) {
                        return true;
                    }
                }
                Quotable::Text(text) => {
                    if text.contains(*ch) {
                        return true;
                    }
                }
            };
        }

        false
    }

    /// Return true if the provided string must be unquoted.
    pub fn requires_unquoted(&self) -> bool {
        quotable_contains_syntax(&self.data, &self.options.unquoted_syntax)
    }
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|chunk| chunk == needle)
}

fn quotable_contains_syntax(data: &Quotable<'_>, syntaxes: &[Syntax]) -> bool {
    for syntax in syntaxes {
        match data {
            Quotable::Bytes(bytes) => {
                match syntax {
                    Syntax::Symbol(symbol) => {
                        let sbytes = symbol.as_bytes();

                        if bytes.windows(sbytes.len()).any(|chunk| chunk == sbytes) {
                            return true;
                        }
                    }
                    Syntax::Pair(open, close) => {
                        let obytes = open.as_bytes();
                        let cbytes = close.as_bytes();

                        if let Some(o) = bytes
                            .windows(obytes.len())
                            .position(|chunk| chunk == obytes)
                        {
                            if bytes[o..]
                                .windows(cbytes.len())
                                .any(|chunk| chunk == cbytes)
                            {
                                return true;
                            }
                        }
                    }
                };
            }
            Quotable::Text(text) => {
                match syntax {
                    Syntax::Symbol(symbol) => {
                        if text.contains(symbol) {
                            return true;
                        }
                    }
                    Syntax::Pair(open, close) => {
                        if let Some(o) = text.find(open) {
                            if text[o..].contains(close) {
                                return true;
                            }
                        }
                    }
                };
            }
        };
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quoter(value: &str) -> Quoter<'_> {
        Quoter::new(value, QuoterOptions::default())
    }

    #[test]
    fn bareword_rejects_empty_and_non_ascii() {
        assert!(quoter("simple123").is_bareword());
        assert!(quoter("foo_bar").is_bareword());

        assert!(!quoter("").is_bareword());
        assert!(!quoter("has space").is_bareword());
        assert!(!quoter("has-dash").is_bareword());

        // Regression: `char as u8` truncated 'Ł' (U+0141) to 0x41 = 'A' and
        // misclassified it as a bareword.
        assert!(!quoter("Ł").is_bareword());
        assert!(!quoter("café").is_bareword());
        assert!(!quoter("naïve").is_bareword());
    }

    #[test]
    fn quoted_requires_single_enclosing_token() {
        // Genuinely quoted: interior has no closing delimiter.
        assert!(quoter("'hello'").is_quoted());
        assert!(quoter("\"hello\"").is_quoted());
        assert!(quoter("''").is_quoted());

        // A lone quote is not "quoted".
        assert!(!quoter("'").is_quoted());
        assert!(!quoter("\"").is_quoted());

        // Regression: concatenated tokens like `'foo'bar'baz'` must NOT be treated
        // as already-quoted (they would silently collapse to `foobarbaz`).
        assert!(!quoter("'foo'bar'baz'").is_quoted());
        assert!(!quoter("'a' 'b'").is_quoted());
    }
}
