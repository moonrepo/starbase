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
        let set = GlobSet::new(["**/*"]).unwrap();

        assert!(set.matches("files/test.js"));
        assert!(!set.matches(".git/cache"));
        assert!(!set.matches("files/.git/cache"));
        assert!(!set.matches(".DS_Store"));
        assert!(!set.matches("files/.DS_Store"));
    }

    #[test]
    fn only_negates_root_node_modules() {
        let set = GlobSet::new(["**/*"]).unwrap();

        assert!(!set.matches("node_modules/foo.js"));
        assert!(!set.matches("node_modules/sub/foo.js"));

        assert!(set.matches("packages/foo/node_modules/bar.js"));
        assert!(set.matches("apps/web/node_modules/dep/index.js"));
    }

    #[test]
    fn errors_with_the_invalid_pattern() {
        for result in [
            GlobSet::new(["files/*", "a:b"]).err(),
            GlobSet::new(["files/*", "!a:b"]).err(),
            GlobSet::new_owned(["files/*", "a:b"]).err(),
        ] {
            match result {
                Some(GlobError::Create { glob, .. }) => assert_eq!(glob, "a:b"),
                other => panic!("expected a create error, got {other:?}"),
            }
        }
    }

    #[cfg(feature = "glob-cache")]
    #[test]
    fn reuses_cached_sets() {
        let list = ["!**/*.ts".to_string(), "files/*".to_string()];
        let a = GlobSet::new_cached(["files/*", "!**/*.ts"]).unwrap();
        let b = GlobSet::new_cached(&list).unwrap();
        let c = GlobSet::new_cached(["files/*"]).unwrap();

        assert!(std::sync::Arc::ptr_eq(&a, &b));
        assert!(!std::sync::Arc::ptr_eq(&a, &c));

        assert!(a.matches("files/test.js"));
        assert!(!a.matches("files/test.ts"));
        assert!(c.matches("files/test.ts"));
    }

    #[test]
    fn matches_parent_traversal() {
        let set = GlobSet::new(["packages/a/.../package.json"]).unwrap();

        assert!(set.matches("packages/a/package.json"));
        assert!(set.matches("packages/package.json"));
        assert!(set.matches("package.json"));

        // Siblings, nested, and outside
        assert!(!set.matches("packages/b/package.json"));
        assert!(!set.matches("packages/a/src/package.json"));
        assert!(!set.matches("other/package.json"));
    }

    #[test]
    fn matches_parent_traversal_from_root() {
        let set = GlobSet::new(["/.../package.json"]).unwrap();

        assert!(set.matches("package.json"));
        assert!(!set.matches("packages/package.json"));
    }

    #[test]
    fn matches_parent_traversal_with_globs() {
        let set = GlobSet::new(["packages/*/.../*.lock"]).unwrap();

        assert!(set.matches("packages/a/yarn.lock"));
        assert!(set.matches("packages/b/Cargo.lock"));
        assert!(set.matches("packages/yarn.lock"));
        assert!(set.matches("yarn.lock"));

        assert!(!set.matches("packages/a/src/yarn.lock"));
        assert!(!set.matches("packages/a/yarn.json"));
        assert!(!set.matches("other/yarn.lock"));
    }

    #[test]
    fn matches_parent_traversal_with_multi_segment_tail() {
        let set = GlobSet::new_owned(["a/b/.../config/app.json"]).unwrap();

        assert!(set.matches("a/b/config/app.json"));
        assert!(set.matches("a/config/app.json"));
        assert!(set.matches("config/app.json"));

        assert!(!set.matches("a/b/app.json"));
        assert!(!set.matches("a/c/config/app.json"));
    }

    #[test]
    fn doesnt_match_parent_traversal_negations() {
        let sets = [
            GlobSet::new(["**/*.json", "!packages/a/.../package.json"]).unwrap(),
            GlobSet::new_owned(["**/*.json", "!packages/a/.../package.json"]).unwrap(),
            GlobSet::new_split(["**/*.json"], ["packages/a/.../package.json"]).unwrap(),
            GlobSet::new_split_owned(["**/*.json"], ["packages/a/.../package.json"]).unwrap(),
        ];

        for set in sets {
            assert!(!set.matches("packages/a/package.json"));
            assert!(!set.matches("packages/package.json"));
            assert!(!set.matches("package.json"));

            assert!(set.matches("packages/b/package.json"));
            assert!(set.matches("packages/a/src/package.json"));
            assert!(set.matches("packages/a/tsconfig.json"));
        }
    }

    #[test]
    fn errors_for_invalid_parent_traversal() {
        for result in [
            GlobSet::new(["files/*", "a/.../b/.../c"]).err(),
            GlobSet::new(["files/*", "!a/..."]).err(),
            GlobSet::new_owned(["files/*", "a/.../**/c"]).err(),
            GlobSet::new_split(["files/*"], ["a/..."]).err(),
        ] {
            assert!(
                matches!(result, Some(GlobError::InvalidParentTraversal { .. })),
                "expected a parent traversal error, got {result:?}"
            );
        }
    }

    #[cfg(feature = "glob-cache")]
    #[test]
    fn matches_cached_parent_traversal() {
        let set = GlobSet::new_cached(["packages/a/.../package.json"]).unwrap();

        assert!(set.matches("packages/a/package.json"));
        assert!(set.matches("package.json"));
        assert!(!set.matches("packages/b/package.json"));
    }

    #[cfg(feature = "glob-cache")]
    #[test]
    fn reuses_cached_split_sets() {
        let a = GlobSet::new_split_cached(["files/*"], ["**/*.ts"]).unwrap();
        let b = GlobSet::new_cached(["!**/*.ts", "files/*"]).unwrap();

        assert!(std::sync::Arc::ptr_eq(&a, &b));

        assert!(a.matches("files/test.js"));
        assert!(!a.matches("files/test.ts"));
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
        assert!(is_glob("..."));
        assert!(is_glob(".../package.json"));
        assert!(is_glob("dir/.../package.json"));
    }

    #[test]
    fn returns_false_when_not_glob() {
        assert!(!is_glob("dir"));
        assert!(!is_glob("file.rs"));
        assert!(!is_glob("dir/file.ts"));
        assert!(!is_glob("dir/dir/file_test.rs"));
        assert!(!is_glob("dir/dirDir/file-ts.js"));
        assert!(!is_glob("dir.../file...rs"));
    }

    #[test]
    fn returns_false_when_escaped_glob() {
        assert!(!is_glob("\\*.rs"));
        assert!(!is_glob("file\\?.js"));
        assert!(!is_glob("folder-\\[id\\]"));
    }

    #[test]
    fn returns_true_when_later_glob_is_not_escaped() {
        assert!(is_glob("\\*.rs/*.rs"));
        assert!(is_glob("folder-\\[id\\]/[name]"));
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

mod parent_traversal {
    use super::*;

    fn expand(pattern: &str) -> Vec<String> {
        expand_parent_traversal(pattern).unwrap()
    }

    fn reason(pattern: &str) -> String {
        match expand_parent_traversal(pattern) {
            Err(GlobError::InvalidParentTraversal { glob, reason }) => {
                assert_eq!(glob, pattern);
                reason
            }
            other => panic!("expected a parent traversal error, got {other:?}"),
        }
    }

    #[test]
    fn detects_segment() {
        assert!(has_parent_traversal("..."));
        assert!(has_parent_traversal(".../package.json"));
        assert!(has_parent_traversal("a/.../package.json"));
        assert!(has_parent_traversal("!a/.../package.json"));
        assert!(has_parent_traversal("/.../package.json"));
        assert!(has_parent_traversal("./.../package.json"));

        assert!(!has_parent_traversal("a/package.json"));
        assert!(!has_parent_traversal("a.../package.json"));
        assert!(!has_parent_traversal("a/....json"));
        assert!(!has_parent_traversal("../package.json"));
    }

    #[test]
    fn returns_as_is_without_segment() {
        assert_eq!(expand("src/**/*.ts"), ["src/**/*.ts"]);
        assert_eq!(expand("!a.../file...rs"), ["!a.../file...rs"]);
    }

    #[test]
    fn expands_to_each_parent() {
        assert_eq!(
            expand("packages/a/.../package.json"),
            [
                "packages/a/package.json",
                "packages/package.json",
                "package.json"
            ]
        );
    }

    #[test]
    fn expands_from_root() {
        assert_eq!(expand(".../package.json"), ["package.json"]);
        assert_eq!(expand("/.../package.json"), ["/package.json"]);
    }

    #[test]
    fn keeps_leading_markers() {
        assert_eq!(expand("/a/.../x"), ["/a/x", "/x"]);
        assert_eq!(expand("./a/.../x"), ["./a/x", "./x"]);
        assert_eq!(expand("!a/b/.../x"), ["!a/b/x", "!a/x", "!x"]);
    }

    #[test]
    fn expands_globs_in_prefix() {
        assert_eq!(
            expand("packages/*/.../x"),
            ["packages/*/x", "packages/x", "x"]
        );
    }

    #[test]
    fn expands_globs_and_segments_in_tail() {
        assert_eq!(expand("a/.../*.lock"), ["a/*.lock", "*.lock"]);
        assert_eq!(
            expand("a/.../config/app.json"),
            ["a/config/app.json", "config/app.json"]
        );
        assert_eq!(
            expand("a/.../{src,lib}/*.ts"),
            ["a/{src,lib}/*.ts", "{src,lib}/*.ts"]
        );
    }

    #[test]
    fn errors_for_multiple_segments() {
        assert_eq!(reason("a/.../b/.../c"), "only 1 is allowed per pattern");
        assert_eq!(reason(".../..."), "only 1 is allowed per pattern");
    }

    #[test]
    fn errors_for_missing_tail() {
        assert_eq!(reason("..."), "a path is required after it");
        assert_eq!(reason("foo/..."), "a path is required after it");
        assert_eq!(reason("foo/.../"), "a path is required after it");
        assert_eq!(reason("!foo/..."), "a path is required after it");
    }

    #[test]
    fn errors_for_globstar_in_tail() {
        assert_eq!(reason(".../**/x"), "globstars are not allowed after it");
        assert_eq!(reason("a/.../src/**"), "globstars are not allowed after it");
        assert_eq!(
            reason("!a/.../**/*.ts"),
            "globstars are not allowed after it"
        );
    }

    #[test]
    fn allows_globstar_in_prefix() {
        assert_eq!(expand("a/**/.../x"), ["a/**/x", "a/x", "x"]);
    }
}

mod walk {
    use super::*;
    use starbase_sandbox::{Sandbox, create_empty_sandbox};
    use std::path::PathBuf;

    pub fn create_traversal_sandbox() -> Sandbox {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("package.json", "");
        sandbox.create_file("packages/package.json", "");
        sandbox.create_file("packages/a/package.json", "");
        sandbox.create_file("packages/a/yarn.lock", "");
        sandbox.create_file("packages/a/src/package.json", "");
        sandbox.create_file("packages/b/package.json", "");
        sandbox.create_file("packages/b/yarn.lock", "");
        sandbox.create_file("yarn.lock", "");
        sandbox
    }

    pub fn sorted(sandbox: &Sandbox, paths: Vec<PathBuf>) -> Vec<String> {
        let mut paths = paths
            .into_iter()
            .map(|path| {
                path.strip_prefix(sandbox.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    #[test]
    fn matches_parent_traversal() {
        let sandbox = create_traversal_sandbox();

        let paths = walk(sandbox.path(), ["packages/a/.../package.json"]).unwrap();

        assert_eq!(
            sorted(&sandbox, paths),
            [
                "package.json",
                "packages/a/package.json",
                "packages/package.json"
            ]
        );
    }

    #[test]
    fn matches_parent_traversal_with_negations() {
        let sandbox = create_traversal_sandbox();

        let paths = walk_files(
            sandbox.path(),
            ["packages/*/.../*.{json,lock}", "!packages/a/.../*.lock"],
        )
        .unwrap();

        assert_eq!(
            sorted(&sandbox, paths),
            [
                "package.json",
                "packages/a/package.json",
                "packages/b/package.json",
                "packages/b/yarn.lock",
                "packages/package.json",
            ]
        );
    }

    #[test]
    fn errors_for_invalid_parent_traversal() {
        let sandbox = create_traversal_sandbox();

        assert!(matches!(
            walk(sandbox.path(), ["a/.../**/x"]),
            Err(GlobError::InvalidParentTraversal { .. })
        ));
        assert!(matches!(
            walk_files(sandbox.path(), ["*", "!a/..."]),
            Err(GlobError::InvalidParentTraversal { .. })
        ));
    }

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

mod walk_fast {
    use super::walk::{create_traversal_sandbox, sorted};
    use super::*;
    use starbase_sandbox::create_empty_sandbox;

    #[test]
    fn default_options_are_all_disabled() {
        let options = GlobWalkOptions::default();

        assert!(!options.cache);
        assert!(!options.ignore_dot_dirs);
        assert!(!options.ignore_dot_files);
        assert!(!options.log_results);
        assert!(!options.only_dirs);
        assert!(!options.only_files);
    }

    #[test]
    fn handles_dot_folders() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("1.txt", "");
        sandbox.create_file("dir/2.txt", "");
        sandbox.create_file(".hidden/3.txt", "");

        let mut paths =
            walk_fast_with_options(sandbox.path(), ["**/*.txt"], GlobWalkOptions::default())
                .unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join(".hidden/3.txt"),
                sandbox.path().join("1.txt"),
                sandbox.path().join("dir/2.txt"),
            ]
        );

        let mut paths = walk_fast_with_options(
            sandbox.path(),
            ["**/*.txt"],
            GlobWalkOptions::default().dot_dirs(true).dot_files(true),
        )
        .unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join("1.txt"),
                sandbox.path().join("dir/2.txt"),
            ]
        );
    }

    #[test]
    fn walks_into_nested_node_modules() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("node_modules/root.js", "");
        sandbox.create_file("packages/a/node_modules/dep.js", "");
        sandbox.create_file("packages/a/src/index.js", "");

        let mut paths =
            walk_fast_with_options(sandbox.path(), ["**/*.js"], GlobWalkOptions::default())
                .unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join("packages/a/node_modules/dep.js"),
                sandbox.path().join("packages/a/src/index.js"),
            ]
        );
    }

    #[test]
    fn matches_depth_bounded_patterns() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("pkg/moon.yml", "");
        sandbox.create_file("pkg/nested/moon.yml", "");
        sandbox.create_file("apps/api/moon.yml", "");
        sandbox.create_file("apps/api/nested/moon.yml", "");

        let mut paths = walk_fast(sandbox.path(), ["*/moon.yml", "apps/*/moon.yml"]).unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join("apps/api/moon.yml"),
                sandbox.path().join("pkg/moon.yml"),
            ]
        );
    }

    #[test]
    fn applies_negations_across_buckets() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("src/a.js", "");
        sandbox.create_file("dist/b.js", "");

        // The `!dist/**` negation must exclude files in `dist`, matching the
        // behavior of the (non-partitioned) `walk`.
        let mut paths = walk_fast(sandbox.path(), ["**/*.js", "!dist/**"]).unwrap();
        paths.sort();

        assert_eq!(paths, vec![sandbox.path().join("src/a.js")]);
    }

    #[test]
    fn caches_results_per_options() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("file.txt", "");
        sandbox.create_file("dir/nested.txt", "");

        let files = walk_fast_with_options(
            sandbox.path(),
            ["*"],
            GlobWalkOptions::default().cache().files(),
        )
        .unwrap();
        let dirs = walk_fast_with_options(
            sandbox.path(),
            ["*"],
            GlobWalkOptions::default().cache().dirs(),
        )
        .unwrap();

        assert_eq!(files, vec![sandbox.path().join("file.txt")]);
        assert_eq!(dirs, vec![sandbox.path().join("dir")]);
    }

    #[test]
    fn doesnt_duplicate_paths_from_overlapping_buckets() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("a/b/x.tsx", "");
        sandbox.create_file("a/y.tsx", "");

        let mut paths = walk_fast(sandbox.path(), ["a/b/*", "a/**/*.tsx"]).unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join("a/b/x.tsx"),
                sandbox.path().join("a/y.tsx")
            ]
        );
    }

    #[test]
    fn ignores_entries_that_cant_be_read() {
        let sandbox = create_empty_sandbox();

        // A missing dir isn't an aborted walk, so it stays an empty result
        let paths = walk_fast(sandbox.path().join("missing"), ["**/*"]).unwrap();

        assert!(paths.is_empty());
    }

    #[test]
    fn doesnt_traverse_excluded_dirs() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("wanted/a.txt", "");
        sandbox.create_file("heavy/b.txt", "");
        sandbox.create_file("heavy/deep/nested/c.txt", "");

        let mut paths = walk_fast(sandbox.path(), ["**/*.txt", "!heavy/**"]).unwrap();
        paths.sort();

        assert_eq!(paths, vec![sandbox.path().join("wanted/a.txt")]);
    }

    #[test]
    fn doesnt_traverse_excluded_dirs_at_any_depth() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("a/b/wanted.txt", "");
        sandbox.create_file("a/.venv/lib/dep.txt", "");
        sandbox.create_file("a/b/.venv/lib/dep.txt", "");

        let mut paths = walk_fast(sandbox.path(), ["**/*.txt", "!**/.venv/**"]).unwrap();
        paths.sort();

        assert_eq!(paths, vec![sandbox.path().join("a/b/wanted.txt")]);
    }

    #[test]
    fn matches_parent_traversal() {
        let sandbox = create_traversal_sandbox();

        let paths = walk_fast(sandbox.path(), ["packages/a/.../package.json"]).unwrap();

        assert_eq!(
            sorted(&sandbox, paths),
            [
                "package.json",
                "packages/a/package.json",
                "packages/package.json"
            ]
        );
    }

    #[test]
    fn matches_parent_traversal_with_negations() {
        let sandbox = create_traversal_sandbox();

        let paths = walk_fast_with_options(
            sandbox.path(),
            ["packages/*/.../*.{json,lock}", "!packages/a/.../*.lock"],
            GlobWalkOptions::default().files(),
        )
        .unwrap();

        assert_eq!(
            sorted(&sandbox, paths),
            [
                "package.json",
                "packages/a/package.json",
                "packages/b/package.json",
                "packages/b/yarn.lock",
                "packages/package.json",
            ]
        );
    }

    #[test]
    fn stops_parent_traversal_at_base_dir() {
        let sandbox = create_traversal_sandbox();

        let paths = walk_fast(sandbox.path().join("packages"), ["a/.../package.json"]).unwrap();

        assert_eq!(
            sorted(&sandbox, paths),
            ["packages/a/package.json", "packages/package.json"]
        );
    }

    #[test]
    fn ignores_missing_parent_traversal_dirs() {
        let sandbox = create_traversal_sandbox();

        let paths = walk_fast(sandbox.path(), ["missing/dir/.../package.json"]).unwrap();

        assert_eq!(paths, [sandbox.path().join("package.json")]);
    }

    #[test]
    fn errors_for_invalid_parent_traversal() {
        let sandbox = create_traversal_sandbox();

        assert!(matches!(
            walk_fast(sandbox.path(), ["a/.../**/x"]),
            Err(GlobError::InvalidParentTraversal { .. })
        ));
        assert!(matches!(
            walk_fast(sandbox.path(), ["*", "!a/..."]),
            Err(GlobError::InvalidParentTraversal { .. })
        ));
    }

    #[test]
    fn traverses_partially_excluded_dirs() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("dist/a.js", "");
        sandbox.create_file("dist/a.min.js", "");
        sandbox.create_file("dist/nested/b.js", "");

        // Only specific files are excluded, so `dist` must still be walked.
        let mut paths = walk_fast(sandbox.path(), ["**/*.js", "!**/*.min.js"]).unwrap();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                sandbox.path().join("dist/a.js"),
                sandbox.path().join("dist/nested/b.js"),
            ]
        );
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
                (
                    "/root".into(),
                    vec![
                        "website".into(),
                        "!packages/cli".into(),
                        "!packages/core-*".into()
                    ]
                ),
                (
                    "/root/packages".into(),
                    vec!["*".into(), "!cli".into(), "!core-*".into()]
                ),
            ])
        );
    }

    #[test]
    fn keeps_negations_in_broad_buckets() {
        // The `!dist/**` negation must land in the `/root` bucket that walks
        // `**/*.js`, not be siphoned off into a `/root/dist` bucket of its own.
        let map = partition_patterns("/root", ["**/*.js", "!dist/**"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([("/root".into(), vec!["**/*.js".into(), "!dist/**".into()])])
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

    #[test]
    fn parent_traversal_buckets_each_parent() {
        let map = partition_patterns("/root", ["packages/a/.../package.json"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["package.json".into()]),
                ("/root/packages".into(), vec!["package.json".into()]),
                ("/root/packages/a".into(), vec!["package.json".into()]),
            ])
        );
    }

    #[test]
    fn parent_traversal_doesnt_merge_into_other_buckets() {
        let map = partition_patterns("/root", ["packages/*", "packages/a/.../x"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["x".into()]),
                ("/root/packages".into(), vec!["*".into(), "x".into()]),
                ("/root/packages/a".into(), vec!["x".into()]),
            ])
        );
    }

    #[test]
    fn parent_traversal_isnt_merged_into() {
        // Otherwise the nested pattern would be merged into the `packages` bucket,
        // and walk every package
        let map = partition_patterns("/root", ["packages/a/.../x", "packages/a/src/**/*"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["x".into()]),
                ("/root/packages".into(), vec!["x".into()]),
                ("/root/packages/a".into(), vec!["x".into()]),
                ("/root/packages/a/src".into(), vec!["**/*".into()]),
            ])
        );
    }

    #[test]
    fn parent_traversal_with_globs_and_negations() {
        let map = partition_patterns("/root", ["a/*/.../*.lock", "!a/b/.../yarn.lock"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                (
                    "/root".into(),
                    vec![
                        "*.lock".into(),
                        "!a/b/yarn.lock".into(),
                        "!a/yarn.lock".into(),
                        "!yarn.lock".into(),
                    ]
                ),
                (
                    "/root/a".into(),
                    vec![
                        "*/*.lock".into(),
                        "*.lock".into(),
                        "!b/yarn.lock".into(),
                        "!yarn.lock".into(),
                    ]
                ),
            ])
        );
    }

    #[test]
    fn parent_traversal_keeps_invalid_patterns() {
        let map = partition_patterns("/root", ["a/.../**/x"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([("/root/a".into(), vec![".../**/x".into()])])
        );
    }

    #[test]
    fn glob_stars() {
        let map = partition_patterns("/root", ["**/file.txt", "dir/sub/**/*", "other/**/*.txt"]);

        assert_eq!(
            map,
            BTreeMap::from_iter([
                ("/root".into(), vec!["**/file.txt".into()]),
                ("/root/dir/sub".into(), vec!["**/*".into()]),
                ("/root/other".into(), vec!["**/*.txt".into()]),
            ])
        );
    }
}
