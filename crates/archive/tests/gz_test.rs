mod utils;

use starbase_archive::codecs::Gz;
use starbase_archive::file::*;
use starbase_archive::{Archiver, strip_compression_suffix};
use starbase_sandbox::create_sandbox;
use starbase_utils::fs;
use std::path::Path;

mod gz {
    use super::*;

    fn file_contents_match(a: &Path, b: &Path) -> bool {
        std::fs::read(a).unwrap() == std::fs::read(b).unwrap()
    }

    #[test]
    fn file() {
        let sandbox = create_sandbox("archives");

        // Pack
        let input = sandbox.path();
        let archive = sandbox.path().join("file.txt.gz");

        let mut archiver = Archiver::new(input, &archive);
        archiver.add_source_file("file.txt", None);
        archiver
            .pack(|file| Ok(FilePacker::new(Gz::new(fs::create_file(file)?))))
            .unwrap();

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(|dir, file| {
                Ok(FileUnpacker::new(
                    dir.join(strip_compression_suffix(&fs::file_name(file))),
                    Gz::new(fs::open_file(file)?),
                ))
            })
            .unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());

        // Compare
        assert!(file_contents_match(
            &input.join("file.txt"),
            &output.join("file.txt")
        ));
    }

    #[test]
    fn file_via_ext() {
        let sandbox = create_sandbox("archives");

        // Pack
        let input = sandbox.path();
        let archive = sandbox.path().join("file.txt.gz");

        let mut archiver = Archiver::new(input, &archive);
        archiver.add_source_file("file.txt", None);
        archiver.pack_from_ext().unwrap();

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack, which derives the output name from the archive name
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver.unpack_from_ext().unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());

        // Compare
        assert!(file_contents_match(
            &input.join("file.txt"),
            &output.join("file.txt")
        ));
    }

    #[test]
    fn file_ignores_prefix() {
        let sandbox = create_sandbox("archives");

        // Pack
        let input = sandbox.path();
        let archive = sandbox.path().join("file.txt.gz");

        let mut archiver = Archiver::new(input, &archive);
        archiver.add_source_file("file.txt", None);
        archiver.set_prefix("some/prefix");
        archiver
            .pack(|file| Ok(FilePacker::new(Gz::new(fs::create_file(file)?))))
            .unwrap();

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("some/prefix");
        archiver
            .unpack(|dir, file| {
                Ok(FileUnpacker::new(
                    dir.join(strip_compression_suffix(&fs::file_name(file))),
                    Gz::new(fs::open_file(file)?),
                ))
            })
            .unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());

        // Compare
        assert!(file_contents_match(
            &input.join("file.txt"),
            &output.join("file.txt")
        ));
    }
}
