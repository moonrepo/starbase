#![allow(deprecated)]

use starbase_sandbox::{create_empty_sandbox, create_sandbox};
use starbase_utils::fs;
use std::fs as std_fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};

mod fs_base {
    use super::*;

    mod copy_dir_all {
        use super::*;

        #[test]
        fn copies_nested_files() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("src/a.txt", "a");
            sandbox.create_file("src/nested/b.txt", "bb");

            let root = sandbox.path();

            fs::copy_dir_all(root.join("src"), root.join("dest")).unwrap();

            assert_eq!(fs::read_file(root.join("dest/a.txt")).unwrap(), "a");
            assert_eq!(fs::read_file(root.join("dest/nested/b.txt")).unwrap(), "bb");
        }

        #[test]
        fn preserves_empty_dir_behavior() {
            let sandbox = create_empty_sandbox();
            let root = sandbox.path();

            std_fs::create_dir_all(root.join("src/empty/nested")).unwrap();

            fs::copy_dir_all(root.join("src"), root.join("dest")).unwrap();

            assert!(!root.join("dest").exists());
        }
    }

    mod find_upwards_until {
        use super::*;

        #[test]
        fn finds_file_before_end_dir() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("nested/root.marker", "");
            sandbox.create_file("nested/deep/file.txt", "");

            let root = sandbox.path();
            let found = fs::find_upwards_until("root.marker", root.join("nested/deep"), root);

            assert_eq!(found, Some(root.join("nested/root.marker")));
        }

        #[test]
        fn stops_at_end_dir() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("root.marker", "");
            sandbox.create_file("nested/deep/file.txt", "");

            let root = sandbox.path();
            let blocked = fs::find_upwards_until(
                "root.marker",
                root.join("nested/deep"),
                root.join("nested"),
            );

            assert_eq!(blocked, None);
        }
    }

    mod read_dir {
        use super::*;

        #[test]
        fn returns_empty_for_missing_dir() {
            let sandbox = create_empty_sandbox();

            assert!(
                fs::read_dir(sandbox.path().join("missing"))
                    .unwrap()
                    .is_empty()
            );
        }
    }

    mod remove_file {
        use super::*;

        #[test]
        fn removes_a_symlink() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");

            let src = sandbox.path().join("source");
            let link = sandbox.path().join("link");

            std::fs::soft_link(&src, &link).unwrap();

            fs::remove_file(&link).unwrap();

            assert!(src.exists());
            assert!(!link.exists());
        }

        #[test]
        fn doesnt_remove_a_broken_symlink() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");

            let src = sandbox.path().join("source");
            let link = sandbox.path().join("link");

            std::fs::soft_link(&src, &link).unwrap();
            fs::remove_file(&src).unwrap();

            fs::remove_file(&link).unwrap();

            assert!(!src.exists());
            assert!(link.symlink_metadata().is_ok()); // exists doesn't work here
        }
    }

    mod remove_link {
        use super::*;

        #[test]
        fn removes_a_symlink() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");

            let src = sandbox.path().join("source");
            let link = sandbox.path().join("link");

            std::fs::soft_link(&src, &link).unwrap();

            fs::remove_link(&link).unwrap();

            assert!(src.exists());
            assert!(!link.exists());
        }

        #[test]
        fn removes_a_broken_symlink() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");

            let src = sandbox.path().join("source");
            let link = sandbox.path().join("link");

            std::fs::soft_link(&src, &link).unwrap();
            fs::remove_file(&src).unwrap();

            fs::remove_link(&link).unwrap();

            assert!(!src.exists());
            assert!(!link.exists());
            assert!(link.symlink_metadata().is_err()); // extra check
        }

        #[test]
        fn doesnt_remove_a_non_symlink() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");

            let src = sandbox.path().join("source");

            fs::remove_link(&src).unwrap();

            assert!(src.exists());
        }
    }

    mod remove_dir_all_except {
        use super::*;

        #[test]
        fn one_depth() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("a", "");
            sandbox.create_file("b", "");
            sandbox.create_file("c", "");
            sandbox.create_file("d", "");

            let root = sandbox.path();

            fs::remove_dir_all_except(root, vec![PathBuf::from("c")]).unwrap();

            assert!(!root.join("a").exists());
            assert!(!root.join("b").exists());
            assert!(root.join("c").exists());
            assert!(!root.join("d").exists());
        }

        #[test]
        fn two_depths() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("a", "");
            sandbox.create_file("b", "");
            sandbox.create_file("c/1", "");
            sandbox.create_file("c/2", "");
            sandbox.create_file("c/3", "");
            sandbox.create_file("d", "");

            let root = sandbox.path();

            fs::remove_dir_all_except(root, vec![PathBuf::from("c/3"), PathBuf::from("d")])
                .unwrap();

            assert!(!root.join("a").exists());
            assert!(!root.join("b").exists());
            assert!(root.join("c").exists());
            assert!(!root.join("c/1").exists());
            assert!(!root.join("c/2").exists());
            assert!(root.join("c/3").exists());
            assert!(root.join("d").exists());
        }

        #[test]
        fn three_depths() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("a", "");
            sandbox.create_file("b", "");
            sandbox.create_file("c/1", "");
            sandbox.create_file("c/2/a", "");
            sandbox.create_file("c/2/b", "");
            sandbox.create_file("c/2/c", "");
            sandbox.create_file("c/3", "");
            sandbox.create_file("d", "");

            let root = sandbox.path();

            fs::remove_dir_all_except(root, vec![PathBuf::from("c/2/b"), PathBuf::from("d")])
                .unwrap();

            assert!(!root.join("a").exists());
            assert!(!root.join("b").exists());
            assert!(root.join("c").exists());
            assert!(!root.join("c/1").exists());
            assert!(root.join("c/2").exists());
            assert!(!root.join("c/2/a").exists());
            assert!(root.join("c/2/b").exists());
            assert!(!root.join("c/2/c").exists());
            assert!(!root.join("c/3").exists());
            assert!(root.join("d").exists());
        }

        #[test]
        fn removes_broken_symlinks() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("source", "");
            sandbox.create_file("keep/file", "");

            let root = sandbox.path();
            let src = root.join("source");
            let link = root.join("broken-link");

            std::fs::soft_link(&src, &link).unwrap();
            fs::remove_file(&src).unwrap();

            fs::remove_dir_all_except(root, vec![PathBuf::from("keep")]).unwrap();

            assert!(root.join("keep/file").exists());
            assert!(link.symlink_metadata().is_err());
        }
    }

    mod remove_dir_stale_contents {
        use super::*;

        #[test]
        fn ignores_missing_dir() {
            let sandbox = create_empty_sandbox();

            let result =
                fs::remove_dir_stale_contents(sandbox.path().join("missing"), Duration::ZERO)
                    .unwrap();

            assert_eq!(result.files_deleted, 0);
            assert_eq!(result.bytes_saved, 0);
        }

        #[test]
        fn removes_stale_files_recursively() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("cache/a.txt", "1234");
            sandbox.create_file("cache/nested/b.txt", "12");
            sandbox.create_file("cache/nested/deeper/c.txt", "1");

            thread::sleep(Duration::from_millis(5));

            let root = sandbox.path();
            let result = fs::remove_dir_stale_contents(root.join("cache"), Duration::ZERO).unwrap();

            assert_eq!(result.files_deleted, 3);
            assert_eq!(result.bytes_saved, 7);
            assert!(!root.join("cache/a.txt").exists());
            assert!(!root.join("cache/nested/b.txt").exists());
            assert!(!root.join("cache/nested/deeper/c.txt").exists());
        }

        #[test]
        fn does_not_count_fresh_files() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("cache/a.txt", "1234");
            sandbox.create_file("cache/nested/b.txt", "12");

            let root = sandbox.path();

            // Nothing is older than an hour, so nothing should be deleted or counted.
            let result =
                fs::remove_dir_stale_contents(root.join("cache"), Duration::from_secs(3600))
                    .unwrap();

            assert_eq!(result.files_deleted, 0);
            assert_eq!(result.bytes_saved, 0);
            assert!(root.join("cache/a.txt").exists());
            assert!(root.join("cache/nested/b.txt").exists());
        }
    }

    mod stale {
        use super::*;

        #[test]
        fn returns_none_for_fresh_modified_file() {
            let sandbox = create_empty_sandbox();
            sandbox.create_file("cache/a.txt", "1234");

            let result = fs::stale(
                sandbox.path().join("cache/a.txt"),
                false,
                Duration::from_secs(60 * 60),
                SystemTime::now(),
            )
            .unwrap();

            assert!(result.is_none());
        }
    }

    mod detect_indent {
        use super::*;

        #[test]
        fn spaces() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(fs::read_file(sandbox.path().join("spaces.js")).unwrap()),
                "  "
            );
        }

        #[test]
        fn spaces_with_comments() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(
                    fs::read_file(sandbox.path().join("spaces-comments.js")).unwrap()
                ),
                "  "
            );
        }

        #[test]
        fn spaces_4() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(fs::read_file(sandbox.path().join("spaces-4.js")).unwrap()),
                "    "
            );
        }

        #[test]
        fn tabs() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(fs::read_file(sandbox.path().join("tabs.js")).unwrap()),
                "\t"
            );
        }

        #[test]
        fn tabs_with_comments() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(
                    fs::read_file(sandbox.path().join("tabs-comments.js")).unwrap()
                ),
                "\t"
            );
        }

        #[test]
        fn tabs_2() {
            let sandbox = create_sandbox("indent");

            assert_eq!(
                fs::detect_indentation(fs::read_file(sandbox.path().join("tabs-2.js")).unwrap()),
                "\t\t"
            );
        }
    }
}
