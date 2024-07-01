use std::env;
use std::path::PathBuf;

/// Return the "home" or "root" path for a vendor-specific environment variable,
/// like `CARGO_HOME`. If the path is relative, will be prefixed with the current
/// working directory.
#[inline]
pub fn get_vendor_home(key: &str) -> Option<PathBuf> {
    match env::var(key) {
        Ok(value) => {
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
/// like `CARGO_HOME`. If the path is relative, will be prefixed with the current
/// working directory. If the variable is not defined, the fallback callback will
/// be executed with the home directory.
#[inline]
pub fn get_vendor_home_resolved<F: FnOnce(PathBuf) -> PathBuf>(key: &str, fallback: F) -> PathBuf {
    match get_vendor_home(key) {
        Some(path) => path,
        None => fallback(dirs::home_dir().expect("Unable to get home directory!")),
    }
}
