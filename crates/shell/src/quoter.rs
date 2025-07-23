use crate::helpers::{get_var_regex, get_var_regex_bytes};
use shell_quote::{Bash, QuoteRefExt};
use std::sync::Arc;

pub use shell_quote::Quotable;

/// Convert a [`Quotable`] instance into a [`String`].
/// If the data is in bytes, it will be lossy converted.
pub fn quotable_into_string(data: Quotable<'_>) -> String {
    match data {
        Quotable::Bytes(bytes) => String::from_utf8_lossy(bytes).into(),
        Quotable::Text(text) => text.to_owned(),
    }
}

fn string_vec(items: &[&str]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
}

fn quote(data: Quotable<'_>) -> String {
    data.quoted(Bash)
}

fn quote_expansion(data: Quotable<'_>) -> String {
    format!("\"{}\"", quotable_into_string(data).replace("\"", "\\\""))
}

/// Options for [`Quoter`].
pub struct QuoterOptions {
    /// List of start and end quotes for strings.
    pub quote_pairs: Vec<(String, String)>,

    /// List of syntax and characters that must be quoted for expansion.
    pub expansion_syntax: Vec<String>,

    /// Handler to apply quoting.
    pub on_quote: Arc<dyn Fn(Quotable<'_>) -> String>,

    /// Handler to apply quoting for expansion.
    pub on_quote_expansion: Arc<dyn Fn(Quotable<'_>) -> String>,
}

impl Default for QuoterOptions {
    fn default() -> Self {
        Self {
            quote_pairs: vec![("\"".into(), "\"".into()), ("'".into(), "'".into())],
            expansion_syntax: string_vec(&[
                "{", "}", // brace
                "~+", "~-", // tilde
                "${", // param
                "$(", // command
                "<(", ">(", // process
                "**", "*", "?", "?(", "*(", "+(", "@(", "!(", // file
            ]),
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
        if !self.is_quoted() && self.requires_expansion() {
            return self.quote_expansion();
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
        // https://www.gnu.org/software/bash/manual/bash.html#Shell-Expansions
        for ch in &self.options.expansion_syntax {
            match &self.data {
                Quotable::Bytes(bytes) => {
                    let chb = ch.as_bytes();

                    if bytes.windows(chb.len()).any(|chunk| chunk == chb) {
                        return true;
                    }
                }
                Quotable::Text(text) => {
                    if text.contains(ch) {
                        return true;
                    }
                }
            };
        }

        match &self.data {
            Quotable::Bytes(bytes) => get_var_regex_bytes().is_match(bytes),
            Quotable::Text(text) => get_var_regex().is_match(text),
        }
    }
}
