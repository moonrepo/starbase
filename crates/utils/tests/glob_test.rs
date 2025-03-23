use starbase_utils::glob::*;

mod globset {
    use super::*;

    #[test]
    fn doesnt_match_when_empty() {
        let list: Vec<String> = vec![];
        let set = GlobSet::new(&list).unwrap();

        assert!(!set.matches("file.ts"));

        // Testing types
        let list: Vec<&str> = vec![];
        let set = GlobSet::new(list).unwrap();

        assert!(!set.matches("file.ts"));
    }

    #[test]
    fn matches_explicit() {
        let set = GlobSet::new(["source"]).unwrap();

        assert!(set.matches("source"));
        assert!(!set.matches("source.ts"));
    }

    #[test]
    fn matches_exprs() {
        let set = GlobSet::new(["files/*.ts"]).unwrap();

        assert!(set.matches("files/index.ts"));
        assert!(set.matches("files/test.ts"));
        assert!(!set.matches("index.ts"));
        assert!(!set.matches("files/index.js"));
        assert!(!set.matches("files/dir/index.ts"));
    }

    #[test]
    fn matches_rel_start() {
        let set = GlobSet::new(["./source"]).unwrap();

        assert!(set.matches("source"));
        assert!(!set.matches("source.ts"));
    }

    #[test]
    fn doesnt_match_negations() {
        let set = GlobSet::new(["files/*", "!**/*.ts"]).unwrap();

        assert!(set.matches("files/test.js"));
        assert!(set.matches("files/test.go"));
        assert!(!set.matches("files/test.ts"));
    }

    #[test]
    fn doesnt_match_negations_using_split() {
        let set = GlobSet::new_split(["files/*"], ["**/*.ts"]).unwrap();

        assert!(set.matches("files/test.js"));
        assert!(set.matches("files/test.go"));
        assert!(!set.matches("files/test.ts"));
    }

    #[test]
    fn doesnt_match_global_negations() {
        let set = GlobSet::new(["files/**/*"]).unwrap();

        assert!(set.matches("files/test.js"));
        assert!(!set.matches("files/node_modules/test.js"));
        assert!(!set.matches("files/.git/cache"));
    }
}

mod is_glob {
    use super::*;

    #[test]
    fn returns_true_when_a_glob() {
        assert!(is_glob("**"));
        assert!(is_glob("**/src/*"));
        assert!(is_glob("src/**"));
        assert!(is_glob("*.ts"));
        assert!(is_glob("file.*"));
        assert!(is_glob("file.{js,ts}"));
        assert!(is_glob("file.[jstx]"));
        assert!(is_glob("file.tsx?"));
    }

    #[test]
    fn returns_false_when_not_glob() {
        assert!(!is_glob("dir"));
        assert!(!is_glob("file.rs"));
        assert!(!is_glob("dir/file.ts"));
        assert!(!is_glob("dir/dir/file_test.rs"));
        assert!(!is_glob("dir/dirDir/file-ts.js"));
    }

    #[test]
    fn returns_false_when_escaped_glob() {
        assert!(!is_glob("\\*.rs"));
        assert!(!is_glob("file\\?.js"));
        assert!(!is_glob("folder-\\[id\\]"));
    }
}

mod split_patterns {
    use super::*;

    #[test]
    fn splits_all_patterns() {
        assert_eq!(
            split_patterns(["*.file", "!neg1.*", "/*.file2", "/!neg2.*", "!/neg3.*"]),
            (
                vec!["*.file", "*.file2"],
                vec!["neg1.*", "neg2.*", "neg3.*"]
            )
        );
    }
}

mod walk {
    use super::*;

    #[test]
    fn fast_and_slow_return_same_list() {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

        let slow = walk(&dir, ["**/*"]).unwrap();
        let fast = walk_fast(&dir, ["**/*"]).unwrap();

        assert_eq!(slow.len(), fast.len());

        let slow = walk(&dir, ["**/*.snap"]).unwrap();
        let fast = walk_fast(&dir, ["**/*.snap"]).unwrap();

        assert_eq!(slow.len(), fast.len());
    }
}

mod walk_files {
    use super::*;

    #[test]
    fn fast_and_slow_return_same_list() {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

        let slow = walk_files(&dir, ["**/*"]).unwrap();
        let fast = walk_fast_with_options(
            &dir,
            ["**/*"],
            GlobWalkOptions {
                only_files: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(slow.len(), fast.len());

        let slow = walk_files(&dir, ["**/*.snap"]).unwrap();
        let fast = walk_fast_with_options(
            &dir,
            ["**/*.snap"],
            GlobWalkOptions {
                only_files: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(slow.len(), fast.len());
    }
}

mod partition_patterns {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn basic() {
        let map = partition_patterns("/root", ["foo/*", "foo/bar/*.txt", "baz/**/*"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root/foo".into(), vec!["*".into(), "bar/*.txt".into()]),
                ("/root/baz".into(), vec!["**/*".into()]),
            ])
        );
    }

    #[test]
    fn no_globs() {
        let map = partition_patterns("/root", ["foo/file.txt", "foo/bar/file.txt", "file.txt"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["file.txt".into()]),
                (
                    "/root/foo".into(),
                    vec!["file.txt".into(), "bar/file.txt".into()]
                ),
            ])
        );
    }

    #[test]
    fn same_root_dir() {
        let map = partition_patterns("/root", ["file.txt", "file.*", "*.{md,mdx}"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([(
                "/root".into(),
                vec!["file.*".into(), "file.txt".into(), "*.{md,mdx}".into()]
            ),])
        );
    }

    #[test]
    fn same_nested_dir() {
        let map = partition_patterns(
            "/root",
            ["nes/ted/file.txt", "nes/ted/file.*", "nes/ted/*.{md,mdx}"],
        );

        assert_eq!(
            map,
            BTreeMap::from_iter([(
                "/root/nes/ted".into(),
                vec!["file.*".into(), "file.txt".into(), "*.{md,mdx}".into()]
            ),])
        );
    }

    #[test]
    fn dot_dir() {
        let map = partition_patterns("/root", [".dir/**/*.yml"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([("/root/.dir".into(), vec!["**/*.yml".into()]),])
        );
    }

    #[test]
    fn with_negations() {
        let map = partition_patterns(
            "/root",
            [
                "./packages/*",
                "!packages/cli",
                "!packages/core-*",
                "website",
            ],
        );

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["website".into()]),
                (
                    "/root/packages".into(),
                    vec!["*".into(), "!cli".into(), "!core-*".into()]
                ),
            ])
        );
    }

    #[test]
    fn global_negations() {
        let map = partition_patterns(
            "/root",
            [
                "foo/file.txt",
                "foo/bar/file.txt",
                "file.txt",
                "!**/node_modules/**",
            ],
        );

        assert_eq!(
            map,
            BTreeMap::from_iter([
                (
                    "/root".into(),
                    vec!["file.txt".into(), "!**/node_modules/**".into(),]
                ),
                (
                    "/root/foo".into(),
                    vec![
                        "file.txt".into(),
                        "bar/file.txt".into(),
                        "!**/node_modules/**".into(),
                    ]
                ),
            ])
        );
    }
}
