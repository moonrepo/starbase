use crate::helpers::normalize_newlines;

pub enum Hook {
    OnChangeDir { command: String, prefix: String },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnChangeDir { .. } => "on change directory",
        }
    }

    pub fn render_template(&self, template: &str) -> String {
        match self {
            Hook::OnChangeDir { command, prefix } => normalize_newlines(template)
                .replace("{command}", command)
                .replace("{prefix}", prefix),
        }
    }
}
