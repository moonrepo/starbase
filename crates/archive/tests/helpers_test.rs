use starbase_archive::{
    escapes_via_symlink, get_full_file_extension, get_supported_archive_extensions,
    is_supported_archive_extension, join_file_name, strip_compression_suffix, strip_path_prefix,
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
        assert_eq!(
            strip_compression_suffix("data.json.bz2".into()),
            "data.json"
        );
        assert_eq!(
            strip_compression_suffix("data.json.bzip2".into()),
            "data.json"
        );
        assert_eq!(strip_compression_suffix("data.json.gz".into()), "data.json");
        assert_eq!(
            strip_compression_suffix("data.json.gzip".into()),
            "data.json"
        );
        assert_eq!(strip_compression_suffix("data.json.xz".into()), "data.json");
        assert_eq!(
            strip_compression_suffix("data.json.zst".into()),
            "data.json"
        );
        assert_eq!(
            strip_compression_suffix("data.json.zstd".into()),
            "data.json"
        );
    }

    #[test]
    fn leaves_name_without_compression_suffix_untouched() {
        assert_eq!(strip_compression_suffix("data.json".into()), "data.json");
        assert_eq!(
            strip_compression_suffix("archive.tar".into()),
            "archive.tar"
        );
    }

    #[test]
    fn strips_only_a_single_suffix() {
        assert_eq!(strip_compression_suffix("data.gz.zst".into()), "data.gz");
    }
}

mod strip_path_prefix_tests {
    use super::*;

    #[test]
    fn strips_literal_prefix() {
        assert_eq!(
            strip_path_prefix(Path::new("some/prefix/file.txt"), "some/prefix"),
            Some(Path::new("file.txt"))
        );
    }

    #[test]
    fn returns_none_when_literal_prefix_doesnt_match() {
        assert_eq!(strip_path_prefix(Path::new("other/file.txt"), "some"), None);
    }

    #[test]
    fn returns_empty_path_when_path_equals_prefix() {
        assert_eq!(
            strip_path_prefix(Path::new("some/prefix"), "some/prefix"),
            Some(Path::new(""))
        );
        assert_eq!(
            strip_path_prefix(Path::new("tool-v1.2.3"), "*"),
            Some(Path::new(""))
        );
    }

    #[test]
    fn star_matches_any_single_component() {
        assert_eq!(
            strip_path_prefix(Path::new("tool-v1.2.3/file.txt"), "*"),
            Some(Path::new("file.txt"))
        );
        assert_eq!(
            strip_path_prefix(Path::new("tool-v1.2.3/bin/tool"), "*"),
            Some(Path::new("bin/tool"))
        );
    }

    #[test]
    fn star_strips_only_one_component() {
        assert_eq!(
            strip_path_prefix(Path::new("a/b/c/d"), "*"),
            Some(Path::new("b/c/d"))
        );
    }

    #[test]
    fn star_mixes_with_literal_components() {
        assert_eq!(
            strip_path_prefix(Path::new("some/tool-v1.2.3/bin/tool"), "some/*/bin"),
            Some(Path::new("tool"))
        );
        assert_eq!(
            strip_path_prefix(Path::new("tool-v1.2.3/lib/file.js"), "*/lib"),
            Some(Path::new("file.js"))
        );
    }

    #[test]
    fn multiple_stars_strip_multiple_components() {
        assert_eq!(
            strip_path_prefix(Path::new("a/b/c/d"), "*/*"),
            Some(Path::new("c/d"))
        );
    }

    #[test]
    fn returns_none_when_literal_component_around_star_doesnt_match() {
        assert_eq!(
            strip_path_prefix(Path::new("foo/x/baz/y"), "foo/*/bar"),
            None
        );
        assert_eq!(strip_path_prefix(Path::new("other/x/y"), "some/*"), None);
    }

    #[test]
    fn returns_none_when_prefix_is_longer_than_path() {
        assert_eq!(strip_path_prefix(Path::new("only"), "*/*"), None);
        assert_eq!(strip_path_prefix(Path::new("a/b"), "a/b/*"), None);
    }

    #[test]
    fn star_must_be_a_whole_component() {
        // Partial patterns are compared literally, not expanded
        assert_eq!(
            strip_path_prefix(Path::new("tool-v1.2.3/file.txt"), "tool-*"),
            None
        );
    }

    #[test]
    fn handles_trailing_separator_in_prefix() {
        assert_eq!(
            strip_path_prefix(Path::new("some/file.txt"), "some/"),
            Some(Path::new("file.txt"))
        );
    }
}
