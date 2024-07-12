use starbase_utils::string_vec;
use std::collections::HashMap;
use std::env;

/// Settings to customize commands and assertions.
pub struct SandboxSettings {
    /// The binary name to use when running binaries in the sandbox.
    pub bin: String,
    /// Environment variables to use when running binaries in the sandbox.
    pub env: HashMap<String, String>,
    /// Filters to apply when filtering log lines from process outputs.
    pub log_filters: Vec<String>,
    /// Timeout when running processes.
    pub timeout: u64,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            bin: env::var("CARGO_BIN_NAME").unwrap_or_default(),
            env: HashMap::default(),
            log_filters: string_vec![
                // Starbase formats
                "[ERROR", "[WARN", "[ WARN", "[INFO", "[ INFO", "[DEBUG", "[TRACE",
            ],
            timeout: 90,
        }
    }
}

impl SandboxSettings {
    pub fn apply_log_filters(&self, input: String) -> String {
        let mut output = String::new();

        for line in input.split('\n') {
            if self.log_filters.iter().all(|f| !line.contains(f)) {
                output.push_str(line);
                output.push('\n');
            }
        }

        output
    }
}
