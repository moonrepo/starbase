use crate::helpers::{get_var_regex, get_var_regex_bytes, quotable_into_string};
use shell_quote::{Bash, QuoteRefExt};
use std::sync::Arc;

pub use shell_quote::Quotable;

fn quote(data: Quotable<'_>) -> String {
    data.quoted(Bash)
}

fn quote_expansion(data: Quotable<'_>) -> String {
    let string = quotable_into_string(data);
    let mut output = String::with_capacity(string.len() + 2);
    output.push('"');

    for c in string.chars() {
        if c == '"' || c == '\\' {
            output.push('\\');
        }
        output.push(c);
    }

    output.push('"');
    output
}

/// Types of syntax to check for to determine quoting.
pub enum Syntax {
    Symbol(String),
    Pair(String, String),
}

/// Options for [`Quoter`].
pub struct QuoterOptions {
    /// List of start and end quotes for strings.
    pub quote_pairs: Vec<(String, String)>,

    /// List of syntax and characters that must be quoted for expansion.
    pub quoted_syntax: Vec<Syntax>,

    /// List of syntax and characters that must not be quoted.
    pub unquoted_syntax: Vec<Syntax>,

    /// Handler to apply quoting.
    pub on_quote: Arc<dyn Fn(Quotable<'_>) -> String>,

    /// Handler to apply quoting for expansion.
    pub on_quote_expansion: Arc<dyn Fn(Quotable<'_>) -> String>,
}

impl Default for QuoterOptions {
    fn default() -> Self {
        Self {
            quote_pairs: vec![("'".into(), "'".into()), ("\"".into(), "\"".into())],
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
            on_quote: Arc::new(quote),
            on_quote_expansion: Arc::new(quote_expansion),
        }
    }
}

/// A utility for quoting a string.
pub struct Quoter<'a> {
    data: Quotable<'a>,
    options: QuoterOptions,
}

impl<'a> Quoter<'a> {
    /// Create a new instance.
    pub fn new(data: impl Into<Quotable<'a>>, options: QuoterOptions) -> Quoter<'a> {
        Self {
            data: data.into(),
            options,
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
        for (sq, eq) in &self.options.quote_pairs {
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

        if self.is_quoted() {
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
        (self.options.on_quote_expansion)(self.data)
    }

    /// Quote the provided string.
    /// This assumes the string is not already quoted.
    pub fn quote(self) -> String {
        (self.options.on_quote)(self.data)
    }

    /// Return true if the provided string requires expansion.
    pub fn requires_expansion(&self) -> bool {
        if quotable_contains_syntax(&self.data, &self.options.quoted_syntax) {
            return true;
        }

        match &self.data {
            Quotable::Bytes(bytes) => get_var_regex_bytes().is_match(bytes),
            Quotable::Text(text) => get_var_regex().is_match(text),
        }
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
