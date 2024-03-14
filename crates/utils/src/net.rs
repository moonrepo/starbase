use crate::fs;
use reqwest::Client;
use std::path::Path;
use url::Url;

pub use crate::net_error::NetError;

/// Download a file from the provided source URL, to the destination file path,
/// using a custom `reqwest` [`Client`].
pub async fn download_from_url_with_client<S: AsRef<str>, D: AsRef<Path>>(
    source_url: S,
    dest_file: D,
    client: &Client,
) -> Result<(), NetError> {
    let source_url = source_url.as_ref();
    let url = Url::parse(source_url).map_err(|error| NetError::UrlParseFailed {
        url: source_url.to_owned(),
        error,
    })?;

    // Fetch the file from the HTTP source
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| NetError::Http {
            error,
            url: source_url.to_owned(),
        })?;

    let status = response.status();

    if status.as_u16() == 404 {
        return Err(NetError::UrlNotFound {
            url: source_url.to_owned(),
        });
    }

    if !status.is_success() {
        return Err(NetError::DownloadFailed {
            url: source_url.to_owned(),
            status: status.to_string(),
        });
    }

    // Write the bytes to our file
    fs::write_file(
        dest_file,
        response.bytes().await.map_err(|error| NetError::Http {
            error,
            url: source_url.to_owned(),
        })?,
    )?;

    Ok(())
}

/// Download a file from the provided source URL, to the destination file path.
pub async fn download_from_url<S: AsRef<str>, D: AsRef<Path>>(
    source_url: S,
    dest_file: D,
) -> Result<(), NetError> {
    download_from_url_with_client(source_url, dest_file, &Client::new()).await
}
