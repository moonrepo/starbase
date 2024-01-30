pub mod fs;
mod fs_error;

#[cfg(feature = "fs-lock")]
mod fs_lock; // Exported from fs

#[cfg(feature = "glob")]
pub mod glob;
#[cfg(feature = "glob")]
mod glob_error;

#[cfg(feature = "json")]
pub mod json;
#[cfg(feature = "json")]
mod json_error;

#[cfg(feature = "toml")]
pub mod toml;
#[cfg(feature = "toml")]
mod toml_error;

#[cfg(feature = "yaml")]
pub mod yaml;
#[cfg(feature = "yaml")]
mod yaml_error;

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
