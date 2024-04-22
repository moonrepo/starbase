use crate::fs;
use reqwest::Client;
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::thread;
use std::time::Duration;
use tracing::trace;
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
        error: Box::new(error),
    })?;

    // Fetch the file from the HTTP source
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| NetError::Http {
            error: Box::new(error),
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
            error: Box::new(error),
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

mod offline {
    use super::*;

    pub fn check_connection(address: SocketAddr, timeout: u64) -> bool {
        trace!("Resolving {address}");

        if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_millis(timeout)) {
            let _ = stream.shutdown(Shutdown::Both);

            return true;
        }

        false
    }

    pub fn check_connection_from_host(host: String, timeout: u64) -> bool {
        // Wrap in a thread because resolving a host to an IP address
        // may take an unknown amount of time. If longer than our timeout,
        // exit early.
        let handle = thread::spawn(move || host.to_socket_addrs().ok());

        thread::sleep(Duration::from_millis(timeout));

        if !handle.is_finished() {
            return false;
        }

        if let Ok(Some(addresses)) = handle.join() {
            for address in addresses {
                if check_connection(address, timeout) {
                    return true;
                }
            }
        }

        false
    }
}

/// Detect if there is an internet connection, or the user is offline.
/// This will first ping Cloudflare and Google DNS IP addresses, which
/// is the fastest approach as they do not need to parse host names.
/// If all of these fail, then we will ping Google, Mozilla, and custom
/// hosts, which is slower, so we wrap them in a timeout.
pub fn is_offline(timeout: u64, custom_hosts: Vec<String>) -> bool {
    trace!(timeout, "Checking for an internet connection");

    // Check these first as they do not need to resolve IP addresses!
    // These typically happen in milliseconds.
    let online = [
        // Cloudflare DNS: https://1.1.1.1/dns/
        SocketAddr::from(([1, 1, 1, 1], 53)),
        SocketAddr::from(([1, 0, 0, 1], 53)),
        // Google DNS: https://developers.google.com/speed/public-dns
        SocketAddr::from(([8, 8, 8, 8], 53)),
        SocketAddr::from(([8, 8, 4, 4], 53)),
    ]
    .into_iter()
    .map(|address| thread::spawn(move || offline::check_connection(address, timeout)))
    .any(|handle| handle.join().is_ok_and(|v| v));

    if online {
        trace!("Online!");

        return false;
    }

    // Check these second as they need to resolve IP addresses,
    // which adds unnecessary time and overhead that can't be
    // controlled with a native timeout.
    let mut hosts = vec![
        "clients3.google.com:80".to_owned(),
        "detectportal.firefox.com:80".to_owned(),
        "google.com:80".to_owned(),
    ];

    if !custom_hosts.is_empty() {
        hosts.extend(custom_hosts);
    }

    let online = hosts
        .into_iter()
        .map(|host| thread::spawn(move || offline::check_connection_from_host(host, timeout)))
        .any(|handle| handle.join().is_ok_and(|v| v));

    if online {
        trace!("Online!");

        return false;
    }

    trace!("Offline!!!");

    true
}
