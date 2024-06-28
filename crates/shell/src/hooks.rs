use crate::helpers::{normalize_newlines, NEWLINE};
use crate::shells::Shell;

pub enum Hook {
    OnChangeDir {
        // Don't use a map as we want to preserve order!
        env: Vec<(String, Option<String>)>,
        paths: Vec<String>,
        prefix: String,
    },
}

impl Hook {
    pub fn get_info(&self) -> &str {
        match self {
            Hook::OnChangeDir { .. } => "on change directory",
        }
    }

    pub fn render_template<S: Shell>(&self, shell: &S, template: &str, indent: &str) -> String {
        match self {
            Hook::OnChangeDir { env, paths, prefix } => {
                if env.is_empty() && paths.is_empty() {
                    return "".into();
                }

                let env = if env.is_empty() {
                    indent.into()
                } else {
                    env.iter()
                        .map(|(key, value)| {
                            let result = shell.format_env(key, value.as_deref());

                            if indent.is_empty() {
                                result
                            } else {
                                format!("{indent}{result}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(NEWLINE)
                };

                let path = if paths.is_empty() {
                    indent.into()
                } else {
                    let result = shell.format_path_set(paths);

                    if indent.is_empty() {
                        result
                    } else {
                        result
                            .lines()
                            .map(|line| format!("{indent}{line}"))
                            .collect::<Vec<_>>()
                            .join(NEWLINE)
                    }
                };

                normalize_newlines(template)
                    .replace("{prefix}", prefix)
                    .replace("{export_env}", &env)
                    .replace("{export_path}", &path)
            }
        }
    }
}
