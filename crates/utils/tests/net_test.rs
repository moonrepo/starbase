use starbase_sandbox::create_empty_sandbox;
use starbase_utils::net;
use std::collections::HashMap;

fn headers_with_custom_header() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "X-Proto-Test-Header".to_string(),
        "proto-starbase-headers-test".to_string(),
    );
    headers
}

mod download {
    use super::*;

    #[test]
    fn checks_online() {
        assert!(!net::is_offline_with_options(Default::default()));
    }

    #[should_panic(expected = "UrlParseFailed")]
    #[tokio::test]
    async fn errors_invalid_url() {
        let sandbox = create_empty_sandbox();

        net::download_from_url(
            "raw.githubusercontent.com/moonrepo/starbase/master/README.md",
            sandbox.path().join("README.md"),
        )
        .await
        .unwrap();
    }

    #[should_panic(expected = "UrlNotFound")]
    #[tokio::test]
    async fn errors_not_found() {
        let sandbox = create_empty_sandbox();

        net::download_from_url(
            "https://raw.githubusercontent.com/moonrepo/starbase/master/UNKNOWN_FILE.md",
            sandbox.path().join("README.md"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn downloads_a_file() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("README.md");

        assert!(!dest_file.exists());

        net::download_from_url(
            "https://raw.githubusercontent.com/moonrepo/starbase/master/README.md",
            &dest_file,
        )
        .await
        .unwrap();

        assert!(dest_file.exists());
        assert_ne!(dest_file.metadata().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn downloads_with_options_headers() {
        let sandbox = create_empty_sandbox();
        let dest_file = sandbox.path().join("headers.json");

        net::download_from_url_with_options(
            "https://httpbin.org/headers",
            &dest_file,
            net::DownloadOptions {
                downloader: Some(Box::new(
                    net::DefaultDownloader::new_with_headers(headers_with_custom_header()).unwrap(),
                )),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let body = std::fs::read_to_string(&dest_file).unwrap();
        assert!(
            body.contains("proto-starbase-headers-test"),
            "expected response to contain custom header value, got: {body}"
        );
    }
}
