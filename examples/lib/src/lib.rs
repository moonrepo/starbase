use starbase_utils::fs;
use std::path::PathBuf;
use tracing::debug;

pub fn create_file() -> miette::Result<()> {
    let file = PathBuf::from("temp/test");

    debug!(file = ?file, "Creating file...");

    fs::write_file(&file, "some contents").unwrap();

    debug!(file = ?file, "Created file!");

    Ok(())
}
