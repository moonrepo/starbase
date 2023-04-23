#[cfg(feature = "tar")]
pub mod tar;

#[cfg(feature = "zip")]
pub mod zip;

mod archive;
mod error;
mod tree_differ;

pub use archive::*;
pub use error::*;
pub use tree_differ::*;

// Use native path utils to join the paths, so we can ensure
// the parts are joined correctly within the archive!
pub fn join_file_name(parts: &[&str]) -> String {
    Vec::from(parts)
        .iter()
        .filter(|p| !p.is_empty())
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .to_string()
}
