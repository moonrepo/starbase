use starbase_utils::path;

mod normalize_separators {
    use super::*;

    #[test]
    #[cfg(not(windows))]
    fn normalizes_windows_separators_on_unix() {
        assert_eq!(path::normalize_separators("foo\\bar\\baz"), "foo/bar/baz");
    }

    #[test]
    #[cfg(windows)]
    fn normalizes_unix_separators_on_windows() {
        assert_eq!(path::normalize_separators("foo/bar/baz"), "foo\\bar\\baz");
    }

    #[test]
    #[cfg(not(windows))]
    fn leaves_unix_paths_unchanged_on_unix() {
        assert_eq!(path::normalize_separators("foo/bar/baz"), "foo/bar/baz");
    }

    #[test]
    #[cfg(windows)]
    fn leaves_windows_paths_unchanged_on_windows() {
        assert_eq!(path::normalize_separators("foo\\bar\\baz"), "foo\\bar\\baz");
    }
}

mod standardize_separators {
    use super::*;

    #[test]
    fn converts_backslashes_to_forward_slashes() {
        assert_eq!(path::standardize_separators("foo\\bar\\baz"), "foo/bar/baz");
    }

    #[test]
    fn leaves_forward_slashes_unchanged() {
        assert_eq!(path::standardize_separators("foo/bar/baz"), "foo/bar/baz");
    }
}
