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
