use starbase_archive::{
    escapes_via_symlink, get_full_file_extension, get_supported_archive_extensions,
    is_supported_archive_extension, join_file_name, strip_compression_suffix,
};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

mod symlink_guard_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn flags_writes_through_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "starbase-symlink-guard-{}-{}",
            std::process::id(),
            UNIX_EPOCH.elapsed().unwrap_or_default().as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();

        // A plain nested path inside the root is safe.
        assert!(!escapes_via_symlink(&root, &root.join("safe/file.txt")));

        // Plant `evil -> <outside>`, mimicking an earlier archive entry; a write
        // through it must be rejected.
        let outside = std::env::temp_dir();
        symlink(&outside, root.join("evil")).unwrap();
        assert!(escapes_via_symlink(&root, &root.join("evil/passwd")));

        // A path that isn't under the root at all is also rejected.
        assert!(escapes_via_symlink(&root, &outside.join("elsewhere")));

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn allows_target_equal_to_root() {
        let root = std::env::temp_dir();

        // The root itself has an empty relative path, so there are no
        // components to traverse and nothing is flagged.
        assert!(!escapes_via_symlink(&root, &root));
    }
}

mod join_file_name_tests {
    use super::*;

    #[test]
    fn joins_parts_with_slash() {
        assert_eq!(join_file_name(["a", "b", "c"]), "a/b/c");
    }

    #[test]
    fn drops_empty_parts() {
        assert_eq!(join_file_name(["a", "", "b", "", "c"]), "a/b/c");
    }

    #[test]
    fn returns_empty_string_when_all_parts_empty() {
        assert_eq!(join_file_name(["", "", ""]), "");
    }

    #[test]
    fn handles_empty_iterator() {
        assert_eq!(join_file_name(Vec::<String>::new()), "");
    }

    #[test]
    fn accepts_owned_strings() {
        let parts = vec![
            "some".to_owned(),
            "prefix".to_owned(),
            "file.txt".to_owned(),
        ];

        assert_eq!(join_file_name(parts), "some/prefix/file.txt");
    }

    #[test]
    fn single_part_has_no_separator() {
        assert_eq!(join_file_name(["file.txt"]), "file.txt");
    }
}

mod get_full_file_extension_tests {
    use super::*;

    #[test]
    fn returns_compound_archive_extension() {
        assert_eq!(
            get_full_file_extension(Path::new("bundle.tar.gz")),
            Some("tar.gz".to_owned())
        );
        assert_eq!(
            get_full_file_extension(Path::new("bundle.tar.zst")),
            Some("tar.zst".to_owned())
        );
    }

    #[test]
    fn returns_single_archive_extension() {
        assert_eq!(
            get_full_file_extension(Path::new("bundle.zip")),
            Some("zip".to_owned())
        );
    }

    #[test]
    fn prefers_full_extension_over_bare_suffix() {
        // `tar.gz` must win over `gz`, since it comes first in the list.
        assert_eq!(
            get_full_file_extension(Path::new("path/to/bundle.tar.gz")),
            Some("tar.gz".to_owned())
        );
    }

    #[test]
    fn falls_back_to_last_extension_for_unsupported_format() {
        assert_eq!(
            get_full_file_extension(Path::new("notes.txt")),
            Some("txt".to_owned())
        );
    }

    #[test]
    fn returns_none_without_extension() {
        assert_eq!(get_full_file_extension(Path::new("README")), None);
    }
}

mod get_supported_archive_extensions_tests {
    use super::*;

    #[test]
    fn includes_common_extensions() {
        let exts = get_supported_archive_extensions();

        for ext in [
            "tar", "zip", "tar.bz2", "tar.gz", "tgz", "tar.xz", "tar.zst", "bz2", "gz", "xz", "zst",
        ] {
            assert!(exts.contains(&ext.to_owned()), "missing {ext}");
        }
    }

    #[test]
    fn extensions_have_no_leading_dot() {
        for ext in get_supported_archive_extensions() {
            assert!(!ext.starts_with('.'), "{ext} should not start with a dot");
        }
    }

    #[test]
    fn compound_extensions_precede_their_bare_suffix() {
        // Any entry whose suffix matches another entry must come first, so
        // `get_full_file_extension` resolves to the most specific match.
        let exts = get_supported_archive_extensions();

        let index_of = |needle: &str| exts.iter().position(|e| e == needle).unwrap();

        assert!(index_of("tar.bz2") < index_of("bz2"));
        assert!(index_of("tar.gz") < index_of("gz"));
        assert!(index_of("tar.xz") < index_of("xz"));
        assert!(index_of("tar.zst") < index_of("zst"));
        assert!(index_of("tar.zstd") < index_of("zstd"));
    }
}

mod is_supported_archive_extension_tests {
    use super::*;

    #[test]
    fn true_for_supported_extensions() {
        assert!(is_supported_archive_extension(Path::new("bundle.tar.gz")));
        assert!(is_supported_archive_extension(Path::new("bundle.zip")));
        assert!(is_supported_archive_extension(Path::new("a/b/bundle.tgz")));
    }

    #[test]
    fn false_for_unsupported_extensions() {
        assert!(!is_supported_archive_extension(Path::new("notes.txt")));
        assert!(!is_supported_archive_extension(Path::new("README")));
    }

    #[test]
    fn false_when_extension_is_only_a_substring() {
        // Ends with `gz` but not `.gz`, so it must not match.
        assert!(!is_supported_archive_extension(Path::new("bundlegz")));
    }
}

mod strip_compression_suffix_tests {
    use super::*;

    #[test]
    fn strips_known_suffixes() {
        assert_eq!(strip_compression_suffix("data.json.bz2"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.bzip2"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.gz"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.gzip"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.xz"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.zst"), "data.json");
        assert_eq!(strip_compression_suffix("data.json.zstd"), "data.json");
    }

    #[test]
    fn leaves_name_without_compression_suffix_untouched() {
        assert_eq!(strip_compression_suffix("data.json"), "data.json");
        assert_eq!(strip_compression_suffix("archive.tar"), "archive.tar");
    }

    #[test]
    fn strips_only_a_single_suffix() {
        assert_eq!(strip_compression_suffix("data.gz.zst"), "data.gz");
    }
}
