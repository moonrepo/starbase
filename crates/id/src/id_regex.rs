use regex::Regex;
use std::sync::LazyLock;

// We need to support all Unicode alphanumeric characters and `\w` is too broad,
// as it includes punctuation and other characters, so we need to be explicit
// with our Unicode character classes.
// https://docs.rs/regex/latest/regex/#perl-character-classes-unicode-friendly

#[doc(hidden)]
pub static ALNUM: &str = r"\p{Alphabetic}\p{M}\p{Join_Control}\d";

#[doc(hidden)]
pub static SYMBOLS: &str = r"/\._-";

/// Pattern that all identifiers are matched against. Supports unicode alphanumeric
/// characters, forward slash `/`, period `.`, underscore `_`, and dash `-`.
/// A leading `@` is supported to support npm package names.
pub static ID_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(format!("^(@?[{ALNUM}{SYMBOLS}]*)$").as_str()).unwrap());

/// Pattern that removes unsupported characters from an identifier.
pub static ID_CLEAN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(format!("[^{ALNUM}{SYMBOLS}]+").as_str()).unwrap());
