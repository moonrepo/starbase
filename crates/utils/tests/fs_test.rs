#![allow(deprecated)]

use starbase_sandbox::{create_empty_sandbox, create_sandbox};
use starbase_utils::fs;
use std::path::PathBuf;

mod fs_base {
    use super::*;

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
