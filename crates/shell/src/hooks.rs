pub enum Statement<'data> {
    ModifyPath {
        paths: &'data [String],
        key: Option<&'data str>,
        orig_key: Option<&'data str>,
    },
    #[deprecated = "Use `ModifyPath` instead."]
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
    OnChangeDir { command: String, function: String },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnChangeDir { .. } => "on change directory",
        }
    }
}
