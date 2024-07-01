use std::env;
use std::path::PathBuf;

/// Return the `PATH` environment variable as a list of [`PathBuf`]s.
#[inline]
pub fn paths() -> Vec<PathBuf> {
    let Some(path) = env::var_os("PATH") else {
        return vec![];
    };

    env::split_paths(&path).collect::<Vec<_>>()
}

/// Return an environment variable is a boolean value. If the value is a `1`, `true`,
/// `yes`, `on`, or `enable`, return true, otherwise return false for all other cases.
pub fn bool_var(key: &str) -> bool {
    match env::var(key) {
        Ok(value) => {
            let value = value.to_lowercase();

            value == "1" || value == "true" || value == "yes" || value == "on" || value == "enable"
        }
        Err(_) => false,
    }
}

/// Return an environment variable with a path-like value, that will be converted
/// to an absolute [`PathBuf`]. If the path is relative, it will be prefixed with
/// the current working directory.
#[inline]
pub fn path_var(key: &str) -> Option<PathBuf> {
    match env::var(key) {
        Ok(value) => {
            if value.is_empty() {
                return None;
            }

            let path = PathBuf::from(value);

            Some(if path.is_absolute() {
                path
            } else {
                env::current_dir()
                    .expect("Unable to get working directory!")
                    .join(path)
            })
        }
        Err(_) => None,
    }
}

/// Return the "home" or "root" path for a vendor-specific environment variable,
/// like `CARGO_HOME`. If the path is relative, it will be prefixed with the current
/// working directory. If the variable is not defined, the fallback function will
/// be called with the home directory.
#[inline]
pub fn vendor_home_var<F: FnOnce(PathBuf) -> PathBuf>(key: &str, fallback: F) -> PathBuf {
    match path_var(key) {
        Some(path) => path,
        None => fallback(dirs::home_dir().expect("Unable to get home directory!")),
    }
}
