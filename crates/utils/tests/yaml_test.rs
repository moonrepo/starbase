use serde_yaml::Value;
use starbase_sandbox::{assert_snapshot, create_sandbox};
use starbase_utils::{fs, yaml};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;

mod editor_config {
    use super::*;

    pub fn append_editor_config(root: &Path, data: &str) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(root.join(".editorconfig"))
            .unwrap();

        writeln!(file, "\n\n{data}").unwrap();
    }

    #[test]
    fn uses_defaults_when_no_config() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.yaml");
        let data: Value = yaml::read_file(&path).unwrap();

        yaml::write_file_with_config(&path, &data).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_change_space_indent() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.yaml");

        append_editor_config(
            sandbox.path(),
            "[*.yaml]\nindent_style = space\nindent_size = 8",
        );

        let data: Value = yaml::read_file(&path).unwrap();

        yaml::write_file_with_config(&path, &data).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_change_tab_indent() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.yaml");

        append_editor_config(sandbox.path(), "[*.yaml]\nindent_style = tab");

        let data: Value = yaml::read_file(&path).unwrap();

        yaml::write_file_with_config(&path, &data).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_enable_trailing_line() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.yaml");

        append_editor_config(sandbox.path(), "[*.yaml]\ninsert_final_newline = true");

        let data: Value = yaml::read_file(&path).unwrap();

        yaml::write_file_with_config(&path, &data).unwrap();

        assert!(fs::read_file(&path).unwrap().ends_with('\n'));
    }

    #[test]
    fn can_disable_trailing_line() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.yaml");

        append_editor_config(sandbox.path(), "[*.yaml]\ninsert_final_newline = false");

        let data: Value = yaml::read_file(&path).unwrap();

        yaml::write_file_with_config(&path, &data).unwrap();

        assert!(!fs::read_file(&path).unwrap().ends_with('\n'));
    }
}
