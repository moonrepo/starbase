use crate::fs::{self, FsError};
use async_trait::async_trait;
use reqwest::{Client, Response};
use std::cmp;
use std::fmt::Debug;
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::thread;
use std::time::Duration;
use tracing::{instrument, trace};
use url::Url;

pub use crate::net_error::NetError;

#[async_trait]
pub trait Downloader: Send {
    async fn download(&self, url: Url) -> Result<Response, NetError>;
}

pub type BoxedDownloader = Box<dyn Downloader>;

#[derive(Default)]
pub struct DefaultDownloader {
    client: reqwest::Client,
}

#[async_trait]
impl Downloader for DefaultDownloader {
    async fn download(&self, url: Url) -> Result<Response, NetError> {
        self.client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| NetError::Http {
                error: Box::new(error),
                url: url.to_string(),
            })
    }
}

pub type OnChunkFn = Box<dyn Fn(u64, u64) + Send>;

#[derive(Default)]
pub struct DownloadOptions {
    pub downloader: Option<BoxedDownloader>,
    pub on_chunk: Option<OnChunkFn>,
}

/// Download a file from the provided source URL, to the destination file path,
/// using custom options.
#[instrument(name = "download_from_url", skip(options))]
pub async fn download_from_url_with_options<S: AsRef<str> + Debug, D: AsRef<Path> + Debug>(
    source_url: S,
    dest_file: D,
    options: DownloadOptions,
) -> Result<(), NetError> {
    let source_url = source_url.as_ref();
    let dest_file = dest_file.as_ref();
    let downloader = options
        .downloader
        .unwrap_or_else(|| Box::new(DefaultDownloader::default()));

    let handle_fs_error = |error: std::io::Error| FsError::Write {
        path: dest_file.to_path_buf(),
        error: Box::new(error),
    };
    let handle_net_error = |error: reqwest::Error| NetError::Http {
        error: Box::new(error),
        url: source_url.to_owned(),
    };

    trace!(
        source_url,
        dest_file = ?dest_file,
        "Downloading file from remote URL to local file",
    );

    // Fetch the file from the HTTP source
    let mut response = downloader
        .download(
            Url::parse(source_url).map_err(|error| NetError::UrlParseFailed {
                url: source_url.to_owned(),
                error: Box::new(error),
            })?,
        )
        .await?;
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

    // Wrap in a closure so that we can capture the error and cleanup
    let do_write = || async {
        let mut file = fs::create_file(dest_file)?;

        // Write the bytes in chunks
        match options.on_chunk {
            Some(on_chunk) => {
                let total_size = response.content_length().unwrap_or(0);
                let mut current_size: u64 = 0;

                on_chunk(0, total_size);

                while let Some(chunk) = response.chunk().await.map_err(handle_net_error)? {
                    file.write_all(&chunk).map_err(handle_fs_error)?;

                    current_size = cmp::min(current_size + (chunk.len() as u64), total_size);

                    on_chunk(current_size, total_size);
                }
            }
            _ => {
                let bytes = response.bytes().await.map_err(handle_net_error)?;

                file.write_all(&bytes).map_err(handle_fs_error)?;
            }
        }

        Ok::<(), NetError>(())
    };

    // Cleanup on failure, otherwise the file was only partially written to
    if let Err(error) = do_write().await {
        let _ = fs::remove_file(dest_file);

        return Err(error);
    }

    Ok(())
}

/// Download a file from the provided source URL, to the destination file path,
/// using a custom `reqwest` [`Client`].
pub async fn download_from_url_with_client<S: AsRef<str> + Debug, D: AsRef<Path> + Debug>(
    source_url: S,
    dest_file: D,
    client: &Client,
) -> Result<(), NetError> {
    download_from_url_with_options(
        source_url,
        dest_file,
        DownloadOptions {
            downloader: Some(Box::new(DefaultDownloader {
                client: client.to_owned(),
            })),
            on_chunk: None,
        },
    )
    .await
}

/// Download a file from the provided source URL, to the destination file path.
pub async fn download_from_url<S: AsRef<str> + Debug, D: AsRef<Path> + Debug>(
    source_url: S,
    dest_file: D,
) -> Result<(), NetError> {
    download_from_url_with_options(source_url, dest_file, DownloadOptions::default()).await
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

#[derive(Debug)]
pub struct OfflineOptions {
    pub check_default_hosts: bool,
    pub check_default_ips: bool,
    pub custom_hosts: Vec<String>,
    pub ip_v4: bool,
    pub ip_v6: bool,
    pub timeout: u64,
}

impl Default for OfflineOptions {
    fn default() -> Self {
        Self {
            check_default_hosts: true,
            check_default_ips: true,
            custom_hosts: vec![],
            ip_v4: true,
            ip_v6: true,
            timeout: 1000,
        }
    }
}

/// Detect if there is an internet connection, or the user is offline,
/// using a custom list of hosts.
#[instrument]
pub fn is_offline(timeout: u64, custom_hosts: Vec<String>) -> bool {
    is_offline_with_options(OfflineOptions {
        custom_hosts,
        timeout,
        ..Default::default()
    })
}

/// Detect if there is an internet connection, or the user is offline.
/// This will first ping Cloudflare and Google DNS IP addresses, which
/// is the fastest approach as they do not need to parse host names.
/// If all of these fail, then we will ping Google, Mozilla, and custom
/// hosts, which is slower, so we wrap them in a timeout.
#[instrument]
pub fn is_offline_with_options(options: OfflineOptions) -> bool {
    trace!(
        timeout = options.timeout,
        "Checking for an internet connection"
    );

    // Check these first as they do not need to resolve IP addresses!
    // These typically happen in milliseconds.
    if options.check_default_ips {
        let mut ips = vec![];

        if options.ip_v4 {
            ips.extend([
                // Cloudflare DNS: https://1.1.1.1/dns/
                SocketAddr::from(([1, 1, 1, 1], 53)),
                SocketAddr::from(([1, 0, 0, 1], 53)),
                // Google DNS: https://developers.google.com/speed/public-dns
                SocketAddr::from(([8, 8, 8, 8], 53)),
                SocketAddr::from(([8, 8, 4, 4], 53)),
            ]);
        }

        if options.ip_v6 {
            ips.extend([
                // Cloudflare DNS: https://1.1.1.1/dns/
                SocketAddr::from(([2606, 4700, 4700, 0, 0, 0, 0, 1111], 53)),
                SocketAddr::from(([2606, 4700, 4700, 0, 0, 0, 0, 1001], 53)),
                // Google DNS: https://developers.google.com/speed/public-dns
                SocketAddr::from(([2001, 4860, 4860, 0, 0, 0, 0, 8888], 53)),
                SocketAddr::from(([2001, 4860, 4860, 0, 0, 0, 0, 8844], 53)),
            ]);
        }

        let online = ips
            .into_iter()
            .map(|address| {
                thread::spawn(move || offline::check_connection(address, options.timeout))
            })
            .any(|handle| handle.join().is_ok_and(|v| v));

        if online {
            trace!("Online!");

            return false;
        }
    }

    // Check these second as they need to resolve IP addresses,
    // which adds unnecessary time and overhead that can't be
    // controlled with a native timeout.
    let mut hosts = vec![];

    if options.check_default_hosts {
        hosts.extend([
            "clients3.google.com:80".to_owned(),
            "detectportal.firefox.com:80".to_owned(),
            "google.com:80".to_owned(),
        ]);
    }

    if !options.custom_hosts.is_empty() {
        hosts.extend(options.custom_hosts);
    }

    let online = hosts
        .into_iter()
        .map(|host| {
            thread::spawn(move || offline::check_connection_from_host(host, options.timeout))
        })
        .any(|handle| handle.join().is_ok_and(|v| v));

    if online {
        trace!("Online!");

        return false;
    }

    trace!("Offline!!!");

    true
}
