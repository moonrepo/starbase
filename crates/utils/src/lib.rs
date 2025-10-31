/// Utilities for reading and writing environment variables.
pub mod envx;

/// Utilities for reading and writing files and directories.
pub mod fs;
mod fs_error;
#[cfg(feature = "fs-lock")]
mod fs_lock; // Exported from fs

#[cfg(feature = "glob")]
/// Utilities for globbing the file system.
pub mod glob;
#[cfg(all(feature = "glob", feature = "glob-cache"))]
mod glob_cache;
#[cfg(feature = "glob")]
mod glob_error;

#[cfg(feature = "json")]
/// Utilities for parsing and formatting JSON.
pub mod json;
#[cfg(feature = "json")]
mod json_error;

/// Utilities for common network patterns.
#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "net")]
mod net_error;

#[cfg(feature = "toml")]
/// Utilities for parsing and formatting TOML.
pub mod toml;
#[cfg(feature = "toml")]
mod toml_error;

#[cfg(feature = "yaml")]
/// Utilities for parsing and formatting YAML.
pub mod yaml;
#[cfg(feature = "yaml")]
mod yaml_error;

/// Utilities for accessing common OS directories.
pub use dirs;

/// Utilities for handling OS paths.
pub mod path;

/// Create a [`Vec`] of owned [`String`]s.
#[macro_export]
macro_rules! string_vec {
    () => {{
        Vec::<String>::new()
    }};
    ($($item:expr_2021),+ $(,)?) => {{
        vec![
            $( String::from($item), )*
        ]
    }};
}
