use crate::fs::FsError;
use starbase_styles::{Style, Stylize};
use thiserror::Error;

#[derive(Error, Debug)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
pub enum NetError {
    #[cfg_attr(feature = "miette", diagnostic(transparent))]
    #[error(transparent)]
    Fs(#[from] FsError),

    #[cfg_attr(feature = "miette", diagnostic(code(net::http)))]
    #[error("Failed to make HTTP request for {}.", .url.style(Style::Url))]
    Http {
        url: String,

        #[source]
        error: reqwest::Error,
    },

    #[cfg_attr(feature = "miette", diagnostic(code(net::download_failed)))]
    #[error(
        "Failed to download file from {} ({status}).",
        .url.style(Style::Url),
    )]
    DownloadFailed { url: String, status: String },

    #[cfg_attr(feature = "miette", diagnostic(code(net::not_found)))]
    #[error("Unable to download file, the URL {} does not exist.", .url.style(Style::Url))]
    UrlNotFound { url: String },

    #[cfg_attr(feature = "miette", diagnostic(code(net::invalid_url)))]
    #[error("Failed to parse URL {}.", .url.style(Style::Url))]
    UrlParseFailed {
        url: String,

        #[source]
        error: url::ParseError,
    },
}
