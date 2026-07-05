
use starbase_archive::escapes_via_symlink;
use std::fs;
use std::os::unix::fs::symlink;
use std::time::UNIX_EPOCH;

mod symlink_guard_tests {
    use super::*;

    #[test]
    fn flags_writes_through_a_symlinked_parent() {
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
}
