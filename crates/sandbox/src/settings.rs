use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::env;
use std::sync::RwLock;

pub static BIN_NAME: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

pub fn get_bin_name() -> String {
    if let Some(bin) = BIN_NAME.read().unwrap().as_ref() {
        return bin.to_owned();
    }

    env::var("CARGO_BIN_NAME").expect("Missing CARGO_BIN_NAME!")
}

pub fn set_bin_name(name: &str) {
    *BIN_NAME.write().unwrap() = Some(name.to_owned());
}

pub static ENV_VARS: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn set_command_env_vars(vars: HashMap<String, String>) {
    ENV_VARS.write().unwrap().extend(vars);
}
