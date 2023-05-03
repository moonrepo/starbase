use once_cell::sync::Lazy;
use starbase_utils::string_vec;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::RwLock;

pub static BIN_NAME: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

pub fn get_bin_name() -> String {
    if let Some(bin) = BIN_NAME.read().unwrap().as_ref() {
        return bin.to_owned();
    }

    env::var("CARGO_BIN_NAME").expect("Missing CARGO_BIN_NAME!")
}

/// Set the binary name to use when running binaries in the sandbox.
pub fn set_bin_name(name: &str) {
    *BIN_NAME.write().unwrap() = Some(name.to_owned());
}

pub static ENV_VARS: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Set environment variables to use when running binaries in the sandbox.
pub fn set_command_env_vars(vars: HashMap<String, String>) {
    ENV_VARS.write().unwrap().extend(vars);
}

pub static LOG_FILTERS: Lazy<RwLock<HashSet<String>>> = Lazy::new(|| {
    RwLock::new(HashSet::from_iter(string_vec![
        "[ERROR", "[WARN", "[INFO", "[DEBUG", "[TRACE",
    ]))
});

/// Set filters to apply when filtering log lines from process outputs.
pub fn set_output_log_filters(filters: HashSet<String>) {
    LOG_FILTERS.write().unwrap().extend(filters);
}
