use thiserror::Error;

/// ID errors.
#[derive(Error, Debug)]
#[error(
    "Invalid identifier format for `{0}`. May only contain alpha-numeric characters, dashes (-), slashes (/), underscores (_), and periods (.)."
)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
#[cfg_attr(feature = "miette", diagnostic(code(id::invalid_format)))]
pub struct IdError(pub String);
