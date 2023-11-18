#![allow(deprecated)]

use starbase_sandbox::create_empty_sandbox;
use starbase_utils::fs;

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
}
