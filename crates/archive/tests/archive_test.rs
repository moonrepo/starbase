use starbase_archive::zip::{ZipPacker, ZipUnpacker};
use starbase_archive::Archiver;
use starbase_sandbox::{create_empty_sandbox, create_sandbox};

#[test]
fn can_add_files() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.add_source_file("file.txt", None);
    archiver.add_source_file("data.json", Some("data-renamed.json"));
    archiver.add_source_file(sandbox.path().join("folder/nested.txt"), None);
    archiver.add_source_file(
        sandbox.path().join("folder/nested.json"),
        Some("folder/nested-renamed.json"),
    );
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(out.path().join("file.txt").exists());
    assert!(!out.path().join("data.json").exists());
    assert!(out.path().join("data-renamed.json").exists());
    assert!(out.path().join("folder/nested.txt").exists());
    assert!(!out.path().join("folder/nested.json").exists());
    assert!(out.path().join("folder/nested-renamed.json").exists());
}

#[test]
fn can_add_files_with_prefix() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.set_prefix("prefix");
    archiver.add_source_file("file.txt", None);
    archiver.add_source_file("data.json", Some("data-renamed.json"));
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.set_prefix(""); // Remove so we can see it unpacked
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(out.path().join("prefix/file.txt").exists());
    assert!(!out.path().join("prefix/data.json").exists());
    assert!(out.path().join("prefix/data-renamed.json").exists());
}

#[test]
fn can_add_files_with_prefix_and_remove_when_unpacking() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.set_prefix("prefix");
    archiver.add_source_file("file.txt", None);
    archiver.add_source_file("data.json", Some("data-renamed.json"));
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(out.path().join("file.txt").exists());
    assert!(!out.path().join("data.json").exists());
    assert!(out.path().join("data-renamed.json").exists());
}

#[test]
fn can_add_globs() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.add_source_glob("**/*.json", None);
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(!out.path().join("file.txt").exists());
    assert!(!out.path().join("folder/nested/other.txt").exists());

    assert!(out.path().join("data.json").exists());
    assert!(out.path().join("folder/nested.json").exists());
}

#[test]
fn can_add_globs_with_group() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.add_source_glob("**/*.json", Some("group"));
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(!out.path().join("group/file.txt").exists());
    assert!(!out.path().join("group/folder/nested/other.txt").exists());

    assert!(out.path().join("group/data.json").exists());
    assert!(out.path().join("group/folder/nested.json").exists());
}

#[test]
fn can_add_globs_with_group_and_prefix() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.set_prefix("prefix");
    archiver.add_source_glob("**/*.json", Some("group"));
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.set_prefix(""); // Remove so we can see it unpacked
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(!out.path().join("prefix/group/file.txt").exists());
    assert!(!out
        .path()
        .join("prefix/group/folder/nested/other.txt")
        .exists());

    assert!(out.path().join("prefix/group/data.json").exists());
    assert!(out.path().join("prefix/group/folder/nested.json").exists());
}

#[test]
fn can_add_globs_with_prefix_and_remove_when_unpacking() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.set_prefix("prefix");
    archiver.add_source_glob("**/*.json", None);
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(!out.path().join("file.txt").exists());
    assert!(!out.path().join("nested/other.txt").exists());

    assert!(out.path().join("data.json").exists());
    assert!(out.path().join("folder/nested.json").exists());
}

#[test]
fn can_add_globs_with_group_and_prefix_and_remove_when_unpacking() {
    let sandbox = create_sandbox("archives");
    let tarball = sandbox.path().join("out.zip");

    let mut archiver = Archiver::new(sandbox.path(), &tarball);
    archiver.set_prefix("prefix");
    archiver.add_source_glob("**/*.json", Some("group"));
    archiver.pack(ZipPacker::new).unwrap();

    let out = create_empty_sandbox();

    archiver.source_root = out.path();
    archiver.unpack(ZipUnpacker::new).unwrap();

    assert!(!out.path().join("group/file.txt").exists());
    assert!(!out.path().join("group/folder/nested/other.txt").exists());

    assert!(out.path().join("group/data.json").exists());
    assert!(out.path().join("group/folder/nested.json").exists());
}
