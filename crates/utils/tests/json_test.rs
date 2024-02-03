use starbase_sandbox::{assert_snapshot, create_sandbox};
use starbase_utils::json::json as object;
use starbase_utils::{fs, json};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;

mod clean {
    use super::*;

    #[test]
    pub fn bypasses_empty_string() {
        assert_eq!(json::clean(""), "");
    }

    #[test]
    pub fn removes_comments() {
        assert_eq!(
            json::clean(r#"{ "foo": true } // comment"#),
            r#"{ "foo": true }           "#
        );
        assert_eq!(
            json::clean(r#"{ "foo": true /* comment */ }"#),
            r#"{ "foo": true               }"#
        );

        assert_eq!(
            json::clean(r#"{ "foo": true /** comment */ }"#),
            r#"{ "foo": true                }"#
        );

        // TODO
        // assert_eq!(
        //     json::clean(r#"{ "foo": true /** comment **/ }"#),
        //     r#"{ "foo": true                 }"#
        // );
    }
}

mod merge {
    use super::*;

    #[test]
    pub fn merges_fields() {
        let prev = object!({
            "base": null,
            "str": "abc",
            "num": 123,
            "bool": true,
            "arr": [1, 2, 3],
            "obj": {
                "key": 123,
                "nested": {
                    "key2": "abc",
                },
            },
        });
        let next = object!({
            "base": {},
            "str": "xyz",
            "arr": [1, 2, 3, 4, 5, 6],
            "obj": {
                "key": null,
                "sub": {
                    "key3": false
                }
            },
        });

        assert_eq!(
            json::merge(&prev, &next),
            object!({
                "base": {},
                "str": "xyz",
                "num": 123,
                "bool": true,
                "arr": [1, 2, 3, 4, 5, 6],
                "obj": {
                    "key": null,
                    "nested": {
                        "key2": "abc",
                    },
                    "sub": {
                        "key3": false
                    }
                },
            })
        );
    }
}

mod editor_config {
    use super::*;

    pub fn append_editor_config(root: &Path, data: &str) {
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(root.join(".editorconfig"))
            .unwrap();

        writeln!(file, "\n\n{data}").unwrap();
    }

    #[test]
    fn uses_defaults_when_no_config() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), true).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn writes_ugly() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), false).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_change_space_indent() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        append_editor_config(sandbox.path(), "[*.json]\nindent_size = 8");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), true).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_change_tab_indent() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        append_editor_config(sandbox.path(), "[*.json]\nindent_style = tab");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), true).unwrap();

        assert_snapshot!(fs::read_file(&path).unwrap());
    }

    #[test]
    fn can_enable_trailing_line() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        append_editor_config(sandbox.path(), "[*.json]\ninsert_final_newline = true");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), true).unwrap();

        assert!(fs::read_file(&path).unwrap().ends_with('\n'));
    }

    #[test]
    fn can_disable_trailing_line() {
        let sandbox = create_sandbox("editor-config");
        let path = sandbox.path().join("file.json");

        append_editor_config(sandbox.path(), "[*.json]\ninsert_final_newline = false");

        json::write_file_with_config(&path, json::read_file(&path).unwrap(), true).unwrap();

        assert!(!fs::read_file(&path).unwrap().ends_with('\n'));
    }
}
