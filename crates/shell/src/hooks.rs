pub enum Statement<'data> {
    PrependPath {
        paths: &'data [String],
        key: Option<&'data str>,
        orig_key: Option<&'data str>,
    },
    SetEnv {
        key: &'data str,
        value: &'data str,
    },
    UnsetEnv {
        key: &'data str,
    },
}

pub enum Hook {
    OnChangeDir { command: String, prefix: String },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnChangeDir { .. } => "on change directory",
        }
    }
}
