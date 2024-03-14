/// Utilities for reading and writing files and directories.
pub mod fs;
mod fs_error;
#[cfg(feature = "fs-lock")]
mod fs_lock; // Exported from fs

#[cfg(feature = "glob")]
/// Utilities for globbing the file system.
pub mod glob;
#[cfg(feature = "glob")]
mod glob_error;

#[cfg(feature = "json")]
/// Utilities for parsing and formatting JSON, backed by `serde_json`.
pub mod json;
#[cfg(feature = "json")]
mod json_error;

#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "net")]
mod net_error;

#[cfg(feature = "toml")]
/// Utilities for parsing and formatting TOML, backed by `toml`.
pub mod toml;
#[cfg(feature = "toml")]
mod toml_error;

#[cfg(feature = "yaml")]
/// Utilities for parsing and formatting YAML, backed by `serde_yaml`.
pub mod yaml;
#[cfg(feature = "yaml")]
mod yaml_error;

/// Utilities for accessing common OS directories.
pub use dirs;

#[macro_export]
macro_rules! string_vec {
    () => {{
        Vec::<String>::new()
    }};
    ($($item:expr),+ $(,)?) => {{
        vec![
            $( String::from($item), )*
        ]
    }};
}
