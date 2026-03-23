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

pub fn apply_quote(
    value: String,
    quotes: (&str, &str),
    replacements: HashMap<char, &str>,
) -> String {
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

pub fn apply_single_quote(value: String) -> String {
    apply_quote(value, ("'", "'"), HashMap::from_iter([('\'', "\\'")]))
}

pub fn apply_double_quote(value: String) -> String {
    apply_quote(value, ("\"", "\""), default_escape_chars())
}

pub fn do_quote(
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
        fn is_bare(ch: u8) -> bool {
            !ch.is_ascii_whitespace() && (ch.is_ascii_alphanumeric() || ch == b'_')
        }

        match &self.data {
            Quotable::Bytes(bytes) => bytes.iter().all(|ch| is_bare(*ch)),
            Quotable::Text(text) => text.chars().all(|ch| is_bare(ch as u8)),
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
            match &self.data {
                Quotable::Bytes(bytes) => {
                    if bytes.starts_with(sq.as_bytes()) && bytes.ends_with(eq.as_bytes()) {
                        return true;
                    }
                }
                Quotable::Text(text) => {
                    if text.starts_with(sq) && text.ends_with(eq) {
                        return true;
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

        do_quote(
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

        do_quote(self.data, (open, close), &self.options.replacements)
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
