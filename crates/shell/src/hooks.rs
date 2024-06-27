use crate::helpers::NEWLINE;
use crate::shells::Shell;

#[derive(Clone, Default)]
pub struct OnCdHook {
    // Don't use a map as we want to preserve order!
    pub env: Vec<(String, Option<String>)>,
    pub paths: Vec<String>,
    pub prefix: String,
}

impl OnCdHook {
    pub fn render_template<S: Shell>(&self, shell: &S, template: &str, indent: &str) -> String {
        let env = if self.env.is_empty() {
            indent.into()
        } else {
            self.env
                .iter()
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

        let path = if self.paths.is_empty() {
            indent.into()
        } else {
            let result = shell.format_path_set(&self.paths);

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

        #[cfg(windows)]
        let template = template.replace("\n", NEWLINE);

        template
            .replace("{prefix}", &self.prefix)
            .replace("{export_env}", &env)
            .replace("{export_path}", &path)
    }
}
