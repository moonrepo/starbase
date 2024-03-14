use starbase_sandbox::create_empty_sandbox;
use starbase_utils::net;

mod download {
    use super::*;

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
}
