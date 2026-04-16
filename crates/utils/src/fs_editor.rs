use crate::fs::{create_dir_all, read_file};
use crate::fs_error::FsError;
use std::cmp;
use std::fmt::Debug;
use std::path::Path;
use tracing::{instrument, trace};

/// Options for `.editorconfig` integration.
pub struct EditorConfigProps {
    /// Value to append to the end of the file, if not already present.
    pub eof: String,

    /// The indentation string to use for the file.
    pub indent: String,
}

impl EditorConfigProps {
    pub fn apply_eof(&self, data: &mut String) {
        if !self.eof.is_empty() && !data.ends_with(&self.eof) {
            data.push_str(&self.eof);
        }
    }
}

/// Detect the indentation of the provided string, by scanning and comparing each line.
#[instrument(skip(content))]
pub fn detect_indentation<T: AsRef<str>>(content: T) -> String {
    let mut spaces = 0;
    let mut tabs = 0;
    let mut lowest_space_width = 0;
    let mut lowest_tab_width = 0;

    fn count_line_indent(line: &str, indent: char) -> usize {
        let mut line_count = 0;
        let mut line_check = line;

        while let Some(inner) = line_check.strip_prefix(indent) {
            line_count += 1;
            line_check = inner;
        }

        line_count
    }

    for line in content.as_ref().lines() {
        if line.starts_with(' ') {
            let line_spaces = count_line_indent(line, ' ');

            // Throw out odd numbers so comments don't throw us
            if line_spaces % 2 == 1 {
                continue;
            }

            spaces += 1;

            if lowest_space_width == 0 || line_spaces < lowest_space_width {
                lowest_space_width = line_spaces;
            }
        } else if line.starts_with('\t') {
            let line_tabs = count_line_indent(line, '\t');

            tabs += 1;

            if lowest_tab_width == 0 || line_tabs < lowest_tab_width {
                lowest_tab_width = line_tabs;
            }
        } else {
            continue;
        }
    }

    if tabs > spaces {
        "\t".repeat(cmp::max(lowest_tab_width, 1))
    } else {
        " ".repeat(cmp::max(lowest_space_width, 2))
    }
}

/// Load properties from the closest `.editorconfig` file.
#[instrument]
pub fn get_editor_config_props<T: AsRef<Path> + Debug>(
    path: T,
) -> Result<EditorConfigProps, FsError> {
    use ec4rs::property::*;

    let path = path.as_ref();
    let editor_config = ec4rs::properties_of(path).unwrap_or_default();
    let tab_width = editor_config
        .get::<TabWidth>()
        .unwrap_or(TabWidth::Value(4));
    let indent_size = editor_config
        .get::<IndentSize>()
        .unwrap_or(IndentSize::Value(2));
    let indent_style = editor_config.get::<IndentStyle>().ok();
    let insert_final_newline = editor_config
        .get::<FinalNewline>()
        .unwrap_or(FinalNewline::Value(true));

    Ok(EditorConfigProps {
        eof: if matches!(insert_final_newline, FinalNewline::Value(true)) {
            "\n".into()
        } else {
            "".into()
        },
        indent: match indent_style {
            Some(IndentStyle::Tabs) => "\t".into(),
            Some(IndentStyle::Spaces) => match indent_size {
                IndentSize::UseTabWidth => match tab_width {
                    TabWidth::Value(value) => " ".repeat(value),
                },
                IndentSize::Value(value) => " ".repeat(value),
            },
            None => {
                if path.exists() {
                    detect_indentation(read_file(path)?)
                } else {
                    "  ".into()
                }
            }
        },
    })
}

/// Write a file with the provided data to the provided path, while taking the
/// closest `.editorconfig` into account
#[inline]
#[instrument(skip(data))]
pub fn write_file_with_config<D: AsRef<[u8]>>(
    path: impl AsRef<Path> + Debug,
    data: D,
) -> Result<(), FsError> {
    let path = path.as_ref();
    let editor_config = get_editor_config_props(path)?;

    let mut data = unsafe { String::from_utf8_unchecked(data.as_ref().to_vec()) };
    editor_config.apply_eof(&mut data);

    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }

    trace!(file = ?path, "Writing file with .editorconfig");

    std::fs::write(path, data).map_err(|error| FsError::Write {
        path: path.to_path_buf(),
        error: Box::new(error),
    })?;

    Ok(())
}
