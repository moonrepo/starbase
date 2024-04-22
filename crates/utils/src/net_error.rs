use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use thiserror::Error;

#[cfg(not(feature = "miette"))]
#[derive(Error, Debug)]
pub enum NetError {
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[error("Failed to make HTTP request for {}.\n{error}", .url.style(Style::Url))]
    Http {
        url: String,
        #[source]
        error: Box<reqwest::Error>,
    },

    #[error(
        "Failed to download file from {} ({status}).",
        .url.style(Style::Url),
    )]
    DownloadFailed { url: String, status: String },

    #[error("Unable to download file, the URL {} does not exist.", .url.style(Style::Url))]
    UrlNotFound { url: String },

    #[error("Failed to parse URL {}.\n{error}", .url.style(Style::Url))]
    UrlParseFailed {
        url: String,
        #[source]
        error: Box<url::ParseError>,
    },
}

#[cfg(feature = "miette")]
#[derive(Error, Debug, miette::Diagnostic)]
pub enum NetError {
    #[diagnostic(transparent)]
    #[error(transparent)]
    Fs(#[from] Box<FsError>),

    #[diagnostic(code(net::http))]
    #[error("Failed to make HTTP request for {}.", .url.style(Style::Url))]
    Http {
        url: String,
        #[source]
        error: Box<reqwest::Error>,
    },

    #[diagnostic(code(net::download_failed))]
    #[error(
        "Failed to download file from {} ({status}).",
        .url.style(Style::Url),
    )]
    DownloadFailed { url: String, status: String },

    #[diagnostic(code(net::not_found))]
    #[error("Unable to download file, the URL {} does not exist.", .url.style(Style::Url))]
    UrlNotFound { url: String },

    #[diagnostic(code(net::invalid_url))]
    #[error("Failed to parse URL {}.", .url.style(Style::Url))]
    UrlParseFailed {
        url: String,
        #[source]
        error: Box<url::ParseError>,
    },
}
