mod utils;

use starbase_archive::Archiver;
use starbase_archive::gz::*;
use starbase_sandbox::create_sandbox;
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
        archiver.pack(GzPacker::new).unwrap();

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver.unpack(GzUnpacker::new).unwrap();

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
        archiver.pack(GzPacker::new).unwrap();

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack
        let output = sandbox.path().join("out");

        let mut archiver = Archiver::new(&output, &archive);
        archiver.set_prefix("some/prefix");
        archiver.unpack(GzUnpacker::new).unwrap();

        assert!(output.exists());
        assert!(output.join("file.txt").exists());

        // Compare
        assert!(file_contents_match(
            &input.join("file.txt"),
            &output.join("file.txt")
        ));
    }
}
