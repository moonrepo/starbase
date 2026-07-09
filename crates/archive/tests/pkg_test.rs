#![cfg(target_os = "macos")]

use starbase_archive::pkg::*;
use starbase_archive::{ArchiveError, Archiver};
use starbase_sandbox::{Sandbox, create_empty_sandbox, create_sandbox};
use std::path::Path;
use std::process::Command;

fn file_contents_match(a: &Path, b: &Path) -> bool {
    std::fs::read_to_string(a).unwrap() == std::fs::read_to_string(b).unwrap()
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();

    assert!(
        output.status.success(),
        "{:?} failed: {}",
        command.get_program(),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Unlike other formats, packages can't be packed, so create them
// for testing with the same system tools that produce real packages.
fn create_component_pkg(identifier: &str, source_dir: &Path, archive_file: &Path) {
    run(Command::new("pkgbuild")
        .arg("--root")
        .arg(source_dir)
        .arg("--identifier")
        .arg(identifier)
        .arg("--version")
        .arg("1.0")
        .arg("--install-location")
        .arg("/usr/local/test")
        .arg(archive_file));
}

fn create_distribution_pkg(component_pkgs: &[&Path], archive_file: &Path) {
    let mut command = Command::new("productbuild");

    for pkg in component_pkgs {
        command.arg("--package").arg(pkg);
    }

    run(command.arg(archive_file));
}

fn create_pkg_from_fixture() -> (Sandbox, Sandbox) {
    let input = create_sandbox("archives");
    let sandbox = create_empty_sandbox();

    create_component_pkg(
        "com.starbase.test",
        input.path(),
        &sandbox.path().join("out.pkg"),
    );

    (input, sandbox)
}

mod pkg {
    use super::*;

    #[test]
    fn dir() {
        let (input, sandbox) = create_pkg_from_fixture();
        let archive = sandbox.path().join("out.pkg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());
        assert!(output.join("folder/nested.txt").exists());
        assert!(output.join("folder/nested/other.txt").exists());

        // Only the payload contents, not the package metadata
        assert!(!output.join("Payload").exists());
        assert!(!output.join("PackageInfo").exists());
        assert!(!output.join("Bom").exists());

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
        let (input, sandbox) = create_pkg_from_fixture();
        let archive = sandbox.path().join("out.pkg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        let (ext, out_dir) = archiver.unpack_from_ext().unwrap();

        assert_eq!(ext, "pkg");
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
        let (input, sandbox) = create_pkg_from_fixture();
        let archive = sandbox.path().join("out.pkg");
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("folder");
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
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
        let (_input, sandbox) = create_pkg_from_fixture();
        let archive = sandbox.path().join("out.pkg");
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("fake/prefix");

        let error = archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap_err();

        assert!(matches!(error, ArchiveError::MissingArchiveContents { .. }));
    }

    #[test]
    fn errors_for_invalid_package() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("bad.pkg", "not a package");

        let archive = sandbox.path().join("bad.pkg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);

        let error = archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap_err();

        assert!(matches!(
            error,
            ArchiveError::Pkg(inner) if matches!(*inner, PkgError::UnpackFailure { .. })
        ));
    }

    #[test]
    fn errors_for_missing_payload() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("scripts/postinstall", "#!/bin/sh\nexit 0\n");

        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(
                sandbox.path().join("scripts/postinstall"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        // A scripts-only package has no payload at all
        run(Command::new("pkgbuild")
            .arg("--nopayload")
            .arg("--identifier")
            .arg("com.starbase.scripts")
            .arg("--version")
            .arg("1.0")
            .arg("--scripts")
            .arg(sandbox.path().join("scripts"))
            .arg(sandbox.path().join("scripts.pkg")));

        let archive = sandbox.path().join("scripts.pkg");
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);

        let error = archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap_err();

        assert!(matches!(
            error,
            ArchiveError::Pkg(inner) if matches!(*inner, PkgError::MissingPayload { .. })
        ));
    }

    #[test]
    fn preserves_symlinks_and_permissions() {
        let input = create_sandbox("archives");
        input.create_file("bin/tool", "#!/bin/sh\necho hi");

        {
            use std::os::unix::fs::PermissionsExt;

            std::fs::set_permissions(
                input.path().join("bin/tool"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }

        std::os::unix::fs::symlink("bin/tool", input.path().join("tool-link")).unwrap();

        let sandbox = create_empty_sandbox();
        let archive = sandbox.path().join("out.pkg");
        let output = sandbox.path().join("out");

        create_component_pkg("com.starbase.test", input.path(), &archive);

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap();

        // Symlinks are recreated, not followed or dropped
        let link = output.join("tool-link");

        assert!(link.symlink_metadata().unwrap().is_symlink());
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            Path::new("bin/tool"),
            "symlink target should be preserved verbatim"
        );

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
            input.create_file("file.txt", format!("package {i}"));

            create_component_pkg(
                &format!("com.starbase.test{i}"),
                input.path(),
                &sandbox.path().join(format!("out-{i}.pkg")),
            );
        }

        std::thread::scope(|scope| {
            for i in 0..3 {
                let sandbox = &sandbox;

                scope.spawn(move || {
                    let archive = sandbox.path().join(format!("out-{i}.pkg"));
                    let output = sandbox.path().join(format!("out-{i}"));

                    let archiver = Archiver::new(&output, &archive);
                    archiver
                        .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
                        .unwrap();

                    assert_eq!(
                        std::fs::read_to_string(output.join("file.txt")).unwrap(),
                        format!("package {i}")
                    );
                });
            }
        });
    }
}

mod pkg_distribution {
    use super::*;

    #[test]
    fn single_component() {
        let (input, sandbox) = create_pkg_from_fixture();
        let archive = sandbox.path().join("dist.pkg");
        let output = sandbox.path().join("out");

        create_distribution_pkg(&[&sandbox.path().join("out.pkg")], &archive);

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap();

        assert!(output.join("file.txt").exists());
        assert!(output.join("folder/nested/other.txt").exists());

        // Only the payload contents, not the package metadata
        assert!(!output.join("Distribution").exists());
        assert!(!output.join("out.pkg").exists());

        assert!(file_contents_match(
            &input.path().join("file.txt"),
            &output.join("file.txt")
        ));
    }

    #[test]
    fn multiple_components_are_merged() {
        let sandbox = create_empty_sandbox();

        let first = create_empty_sandbox();
        first.create_file("first.txt", "first");
        create_component_pkg(
            "com.starbase.first",
            first.path(),
            &sandbox.path().join("first.pkg"),
        );

        let second = create_empty_sandbox();
        second.create_file("second.txt", "second");
        create_component_pkg(
            "com.starbase.second",
            second.path(),
            &sandbox.path().join("second.pkg"),
        );

        let archive = sandbox.path().join("dist.pkg");
        let output = sandbox.path().join("out");

        create_distribution_pkg(
            &[
                &sandbox.path().join("first.pkg"),
                &sandbox.path().join("second.pkg"),
            ],
            &archive,
        );

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap();

        assert!(output.join("first.txt").exists());
        assert!(output.join("second.txt").exists());
    }

    #[test]
    fn prefix_only_needs_to_exist_in_one_component() {
        let sandbox = create_empty_sandbox();

        let first = create_empty_sandbox();
        first.create_file("folder/inner.txt", "inner");
        create_component_pkg(
            "com.starbase.first",
            first.path(),
            &sandbox.path().join("first.pkg"),
        );

        let second = create_empty_sandbox();
        second.create_file("second.txt", "second");
        create_component_pkg(
            "com.starbase.second",
            second.path(),
            &sandbox.path().join("second.pkg"),
        );

        let archive = sandbox.path().join("dist.pkg");
        let output = sandbox.path().join("out");

        create_distribution_pkg(
            &[
                &sandbox.path().join("first.pkg"),
                &sandbox.path().join("second.pkg"),
            ],
            &archive,
        );

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("folder");
        archiver
            .unpack(|dir, file| Ok(PkgUnpacker::new(dir, file)))
            .unwrap();

        // Contents of the prefixed folder, from the component that has it
        assert!(output.join("inner.txt").exists());
        assert!(!output.join("second.txt").exists());
    }
}
