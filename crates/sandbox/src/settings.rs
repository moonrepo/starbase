use once_cell::sync::Lazy;
use std::env;
use std::sync::RwLock;

static BIN_NAME: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

pub fn get_bin_name() -> String {
    if let Some(bin) = BIN_NAME.read().unwrap().as_ref() {
        return bin.to_owned();
    }

    env::var("CARGO_BIN_NAME").expect("Missing CARGO_BIN_NAME!")
}

pub fn set_bin_name(name: &str) {
    *BIN_NAME.write().unwrap() = Some(name.to_owned());
}
