use assert_fs::prelude::*;
pub use assert_fs::TempDir;
use std::path::{Path, PathBuf};

pub struct Sandbox {
    pub fixture: TempDir,
}

impl Sandbox {
    pub fn path(&self) -> &Path {
        self.fixture.path()
    }
}

pub fn get_fixtures_root() -> PathBuf {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.push("../../tests/fixtures");
    dbg!(&root);
    root.canonicalize().unwrap()
}

pub fn get_fixtures_path<T: AsRef<str>>(name: T) -> PathBuf {
    let path = get_fixtures_root().join(name.as_ref());

    if !path.exists() {
        panic!(
            "{}",
            format!("Fixture {} does no exist.", path.to_string_lossy())
        );
    }

    path
}

pub fn create_temp_dir() -> TempDir {
    TempDir::new().unwrap()
}

pub fn create_sandbox<T: AsRef<str>>(fixture: T) -> Sandbox {
    let temp_dir = create_temp_dir();

    temp_dir
        .copy_from(get_fixtures_path(fixture), &["**/*"])
        .unwrap();

    Sandbox {
        // command: None,
        fixture: temp_dir,
    }
}
