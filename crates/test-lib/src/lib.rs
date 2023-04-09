use starbase_utils::fs;
use std::path::PathBuf;
use tracing::debug;

pub fn create_file() -> miette::Result<()> {
    let file = PathBuf::from("test");

    debug!(file = file.to_str(), "Creating file...");

    fs::write_file(&file, "some contents")?;

    debug!(file = file.to_str(), "Created file!");

    Ok(())
}
