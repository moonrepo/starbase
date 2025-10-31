use starbase_utils::{envx, path};
use std::path::{Path, PathBuf};

/// Locate a fixture on the file system by searching up the directory tree
/// for a `tests/__fixtures__/<fixture>` directory, starting from the current
/// Cargo project root.
pub fn locate_fixture<T: AsRef<str>>(fixture: T) -> PathBuf {
    let fixture = fixture.as_ref();
    let starting_dir = envx::path_var("CARGO_MANIFEST_DIR").expect("Missing CARGO_MANIFEST_DIR!");
    let mut dir: &Path = &starting_dir;

    loop {
        let fixture_path = dir.join("tests").join("__fixtures__").join(fixture);

        if fixture_path.exists() {
            return path::clean(fixture_path);
        }

        // Don't traverse past the root!
        if dir.join("Cargo.lock").exists() {
            break;
        }

        match dir.parent() {
            Some(parent) => {
                dir = parent;
            }
            None => {
                break;
            }
        };
    }

    panic!("Fixture \"{fixture}\" does not exist!");
}
