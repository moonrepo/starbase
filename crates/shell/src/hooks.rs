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
