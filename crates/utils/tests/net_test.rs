use starbase_sandbox::create_empty_sandbox;
use starbase_utils::net::{self, NetError};
use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

fn run_async<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn headers_with_custom_header() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "X-Proto-Test-Header".to_string(),
        "proto-starbase-headers-test".to_string(),
    );
    headers
}

fn read_request(mut stream: &std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 512];

    loop {
        let read = stream.read(&mut buffer).unwrap();

        if read == 0 {
            break;
        }

        request.extend_from_slice(&buffer[..read]);

        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    String::from_utf8(request).unwrap()
}

fn serve_http_once<F>(handler: F) -> String
where
    F: FnOnce(std::net::TcpStream) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());

    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handler(stream);
    });

    url
}

fn serve_response_once(status: &'static str, body: &'static [u8]) -> String {
    serve_http_once(move |mut stream| {
        read_request(&stream);

        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
    })
}

mod offline {
    use super::*;

    #[test]
    fn checks_custom_hosts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let host = listener.local_addr().unwrap().to_string();

        thread::spawn(move || {
            let _ = listener.accept();
        });

        assert!(!net::is_offline_with_options(net::OfflineOptions {
            check_default_hosts: false,
            check_default_ips: false,
            custom_hosts: vec![host],
            timeout: 1000,
            ..Default::default()
        }));
    }

    #[test]
    fn times_out_custom_hosts() {
        assert!(net::is_offline_with_options(net::OfflineOptions {
            check_default_hosts: false,
            check_default_ips: false,
            custom_hosts: vec!["example.invalid:80".into()],
            timeout: 1,
            ..Default::default()
        }));
    }
}

mod download {
    use super::*;

    #[test]
    fn errors_invalid_url() {
        let sandbox = create_empty_sandbox();
        let error = run_async(net::download_from_url(
            "raw.githubusercontent.com/moonrepo/starbase/master/README.md",
            sandbox.path().join("README.md"),
        ))
        .unwrap_err();

        assert!(matches!(error, NetError::UrlParseFailed { .. }));
    }

    #[test]
    fn errors_not_found() {
        let sandbox = create_empty_sandbox();
        let url = serve_response_once("404 Not Found", b"nope");
        let error = run_async(net::download_from_url(
            format!("{url}/UNKNOWN_FILE.md"),
            sandbox.path().join("README.md"),
        ))
        .unwrap_err();

        assert!(matches!(error, NetError::UrlNotFound { .. }));
    }

    #[test]
    fn downloads_a_file() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("README.md");
        let url = serve_response_once("200 OK", b"# starbase\n");

        assert!(!dest_file.exists());

        run_async(net::download_from_url(
            format!("{url}/README.md"),
            &dest_file,
        ))
        .unwrap();

        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "# starbase\n");
    }

    #[test]
    fn downloads_with_options_headers() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("headers.json");
        let (sender, receiver) = mpsc::channel();
        let url = serve_http_once(move |mut stream| {
            let request = read_request(&stream);
            sender.send(request).unwrap();

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
        });

        run_async(net::download_from_url_with_options(
            format!("{url}/headers"),
            &dest_file,
            net::DownloadOptions {
                downloader: Some(Box::new(
                    net::DefaultDownloader::new_with_headers(headers_with_custom_header()).unwrap(),
                )),
                ..Default::default()
            },
        ))
        .unwrap();

        let request = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(request.contains("x-proto-test-header: proto-starbase-headers-test"));
        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "{}");
    }

    #[test]
    fn reports_progress_while_streaming() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("stream.txt");
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_for_callback = Arc::clone(&progress);
        let url = serve_http_once(move |mut stream| {
            read_request(&stream);

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n")
                .unwrap();
            stream.write_all(b"hello ").unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(10));
            stream.write_all(b"world").unwrap();
        });

        run_async(net::download_from_url_with_options(
            format!("{url}/stream.txt"),
            &dest_file,
            net::DownloadOptions {
                on_chunk: Some(Arc::new(move |current, total| {
                    progress_for_callback.lock().unwrap().push((current, total));
                })),
                ..Default::default()
            },
        ))
        .unwrap();

        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "hello world");

        let progress = progress.lock().unwrap();
        assert_eq!(progress.first(), Some(&(0, 11)));
        assert_eq!(progress.last(), Some(&(11, 11)));
    }

    #[test]
    fn removes_partial_file_on_stream_error() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("partial.txt");
        let url = serve_http_once(move |mut stream| {
            read_request(&stream);

            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n")
                .unwrap();
            stream.write_all(b"partial").unwrap();
        });

        let error = run_async(net::download_from_url(
            format!("{url}/partial.txt"),
            &dest_file,
        ))
        .unwrap_err();

        assert!(matches!(error, NetError::Http { .. }));
        assert!(!dest_file.exists());
    }
}
