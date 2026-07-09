#![cfg(target_os = "macos")]

use starbase_archive::dmg::*;
use starbase_archive::{ArchiveError, Archiver};
use starbase_sandbox::{Sandbox, create_empty_sandbox, create_sandbox};
use std::path::Path;
use std::process::Command;

fn file_contents_match(a: &Path, b: &Path) -> bool {
    std::fs::read_to_string(a).unwrap() == std::fs::read_to_string(b).unwrap()
}

// Unlike other formats, dmg images can't be packed, so create
// them for testing with the same tool that unpacks them.
fn create_dmg(source_dir: &Path, archive_file: &Path) {
    let output = Command::new("hdiutil")
        .arg("create")
        .arg("-srcfolder")
        .arg(source_dir)
        .arg("-volname")
        .arg("Test")
        .arg("-fs")
        .arg("HFS+")
        .arg("-format")
        .arg("UDZO")
        .arg("-quiet")
        .arg(archive_file)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "hdiutil create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_dmg_from_fixture() -> (Sandbox, Sandbox) {
    let input = create_sandbox("archives");
    let sandbox = create_empty_sandbox();

    create_dmg(input.path(), &sandbox.path().join("out.dmg"));

    (input, sandbox)
}

mod dmg {
    use super::*;

    #[test]
    fn dir() {
        let (input, sandbox) = create_dmg_from_fixture();
        let archive = sandbox.path().join("out.dmg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
            .unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());
        assert!(output.join("folder/nested.txt").exists());
        assert!(output.join("folder/nested/other.txt").exists());

        assert!(file_contents_match(
            &input.path().join("file.txt"),
            &output.join("file.txt")
        ));
        assert!(file_contents_match(
            &input.path().join("folder/nested/other.txt"),
            &output.join("folder/nested/other.txt")
        ));
    }

    #[test]
    fn dir_via_ext() {
        let (input, sandbox) = create_dmg_from_fixture();
        let archive = sandbox.path().join("out.dmg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        let (ext, out_dir) = archiver.unpack_from_ext().unwrap();

        assert_eq!(ext, "dmg");
        assert_eq!(out_dir, output);
        assert!(output.join("file.txt").exists());
        assert!(output.join("folder/nested/other.txt").exists());

        assert!(file_contents_match(
            &input.path().join("file.txt"),
            &output.join("file.txt")
        ));
    }

    #[test]
    fn dir_with_prefix_thats_removed_when_unpacked() {
        let (input, sandbox) = create_dmg_from_fixture();
        let archive = sandbox.path().join("out.dmg");
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("folder");
        archiver
            .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
            .unwrap();

        assert!(output.exists());
        assert!(output.join("nested.txt").exists());
        assert!(output.join("nested/other.txt").exists());
        assert!(!output.join("file.txt").exists());

        assert!(file_contents_match(
            &input.path().join("folder/nested/other.txt"),
            &output.join("nested/other.txt")
        ));
    }

    #[test]
    fn errors_for_missing_prefix() {
        let (_input, sandbox) = create_dmg_from_fixture();
        let archive = sandbox.path().join("out.dmg");
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("fake/prefix");

        let error = archiver
            .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
            .unwrap_err();

        assert!(matches!(
            error,
            ArchiveError::MissingArchiveContents { .. }
        ));
    }

    #[test]
    fn errors_for_invalid_image() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("bad.dmg", "not a disk image");

        let archive = sandbox.path().join("bad.dmg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);

        let error = archiver
            .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
            .unwrap_err();

        assert!(matches!(
            error,
            ArchiveError::Dmg(inner) if matches!(*inner, DmgError::UnpackFailure { .. })
        ));
    }

    #[test]
    fn preserves_symlinks_and_permissions() {
        let input = create_sandbox("archives");
        input.create_file("bin/tool", "#!/bin/sh\necho hi");

        let bin = input.path().join("bin/tool");
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();

        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }

        std::fs::set_permissions(&bin, perms).unwrap();
        std::os::unix::fs::symlink("bin/tool", input.path().join("tool-link")).unwrap();
        std::os::unix::fs::symlink("/Applications", input.path().join("Applications")).unwrap();

        let sandbox = create_empty_sandbox();
        let archive = sandbox.path().join("out.dmg");
        let output = sandbox.path().join("out");

        create_dmg(input.path(), &archive);

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
            .unwrap();

        // Internal symlinks are recreated, not followed or dropped
        let link = output.join("tool-link");

        assert!(link.symlink_metadata().unwrap().is_symlink());
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            Path::new("bin/tool"),
            "symlink target should be preserved verbatim"
        );

        // External symlinks are also recreated, not deep copied
        let apps = output.join("Applications");

        assert!(apps.symlink_metadata().unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&apps).unwrap(), Path::new("/Applications"));

        // Executable bits are preserved
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = std::fs::metadata(output.join("bin/tool"))
                .unwrap()
                .permissions()
                .mode();

            assert_eq!(mode & 0o755, 0o755);
        }
    }

    #[test]
    fn unpacks_in_parallel() {
        let sandbox = create_empty_sandbox();

        for i in 0..3 {
            let input = create_empty_sandbox();
            input.create_file("file.txt", format!("image {i}"));

            create_dmg(input.path(), &sandbox.path().join(format!("out-{i}.dmg")));
        }

        std::thread::scope(|scope| {
            for i in 0..3 {
                let sandbox = &sandbox;

                scope.spawn(move || {
                    let archive = sandbox.path().join(format!("out-{i}.dmg"));
                    let output = sandbox.path().join(format!("out-{i}"));

                    let archiver = Archiver::new(&output, &archive);
                    archiver
                        .unpack(|dir, file| Ok(DmgUnpacker::new(dir, file)))
                        .unwrap();

                    assert_eq!(
                        std::fs::read_to_string(output.join("file.txt")).unwrap(),
                        format!("image {i}")
                    );
                });
            }
        });
    }
}
