use starbase_archive::TreeDiffer;
use starbase_sandbox::{create_empty_sandbox, Sandbox};
use std::fs::{self, File};

fn create_differ_sandbox() -> Sandbox {
    let sandbox = create_empty_sandbox();

    for i in 0..25 {
        sandbox.create_file(format!("templates/{i}.txt"), i.to_string());
        sandbox.create_file(format!("templates/{i}.md"), i.to_string());
        sandbox.create_file(format!("other/{i}"), i.to_string());
    }

    sandbox
}

#[test]
fn loads_all_files() {
    let sandbox = create_differ_sandbox();
    let differ = TreeDiffer::load(sandbox.path(), ["templates"]).unwrap();

    assert_eq!(differ.files.len(), 50);
}

#[test]
fn loads_using_globs() {
    let sandbox = create_differ_sandbox();
    let differ = TreeDiffer::load(sandbox.path(), ["templates/**/*.md"]).unwrap();

    assert_eq!(differ.files.len(), 25);
}

#[test]
fn removes_stale_files() {
    let sandbox = create_differ_sandbox();
    let mut differ = TreeDiffer::load(sandbox.path(), ["templates"]).unwrap();

    // Delete everything, hah
    differ.remove_stale_tracked_files();

    let differ = TreeDiffer::load(sandbox.path(), ["templates"]).unwrap();

    assert_eq!(differ.files.len(), 0);
}

#[test]
fn doesnt_remove_dir_locks() {
    let sandbox = create_empty_sandbox();
    sandbox.create_file(".lock", "123");
    sandbox.create_file("file.txt", "");

    let mut differ = TreeDiffer::load(sandbox.path(), ["**/*"]).unwrap();

    differ.remove_stale_tracked_files();

    assert!(sandbox.path().join(".lock").exists());
    assert!(!sandbox.path().join("file.txt").exists());
}

mod equal_check {
    use super::*;

    #[test]
    fn returns_true_if_equal() {
        let sandbox = create_differ_sandbox();
        let differ = TreeDiffer::load(sandbox.path(), ["templates"]).unwrap();

        let source_path = sandbox.path().join("templates/1.txt");
        fs::write(&source_path, "content").unwrap();
        let mut source = File::open(&source_path).unwrap();

        let dest_path = sandbox.path().join("templates/1.md");
        fs::write(&dest_path, "content").unwrap();
        let mut dest = File::open(&dest_path).unwrap();

        assert!(differ.are_files_equal(&mut source, &mut dest));
    }

    #[test]
    fn returns_false_if_diff_sizes() {
        let sandbox = create_differ_sandbox();
        let differ = TreeDiffer::load(sandbox.path(), ["templates/**/*"]).unwrap();

        let source_path = sandbox.path().join("templates/2.txt");
        fs::write(&source_path, "data").unwrap();
        let mut source = File::open(&source_path).unwrap();

        let dest_path = sandbox.path().join("templates/2.md");
        fs::write(&dest_path, "content").unwrap();
        let mut dest = File::open(&dest_path).unwrap();

        assert!(!differ.are_files_equal(&mut source, &mut dest));
    }

    #[test]
    fn returns_false_if_diff_data() {
        let sandbox = create_differ_sandbox();
        let differ = TreeDiffer::load(sandbox.path(), ["templates"]).unwrap();

        let source_path = sandbox.path().join("templates/3.txt");
        fs::write(&source_path, "cont...").unwrap();
        let mut source = File::open(&source_path).unwrap();

        let dest_path = sandbox.path().join("templates/3.md");
        fs::write(&dest_path, "content").unwrap();
        let mut dest = File::open(&dest_path).unwrap();

        assert!(!differ.are_files_equal(&mut source, &mut dest));
    }
}
