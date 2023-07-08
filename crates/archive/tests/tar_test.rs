mod utils;

use starbase_archive::tar::*;
use starbase_archive::Archiver;
use starbase_sandbox::create_sandbox;
use std::path::Path;

mod tar_gz {
    use super::*;

    generate_tests!(
        "out.tar.gz",
        |_, file| TarPacker::<flate2::write::GzEncoder<std::fs::File>>::new_gz(file, None),
        TarUnpacker::<flate2::write::GzDecoder<std::fs::File>>::new_gz
    );

    #[test]
    fn file2() {
        let sandbox = create_sandbox("archives");

        // Pack
        let input = sandbox.path();
        let archive = sandbox.path().join("test.tar.gz");

        let mut archiver = Archiver::new(input, &archive);
        archiver.add_source_file("file.txt", None);
        archiver
            .pack(|_, file| TarPacker::<std::fs::File>::new_raw(file))
            .unwrap();

        dbg!("size", archive.metadata().unwrap().len());

        assert!(archive.exists());
        assert_ne!(archive.metadata().unwrap().len(), 0);

        // Unpack
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &archive);
        archiver
            .unpack(TarUnpacker::<std::fs::File>::new_raw)
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

mod tar_xz {
    use super::*;

    generate_tests!(
        "out.tar.xz",
        |_, file| TarPacker::<xz2::read::XzEncoder<std::fs::File>>::new_gz(file, None),
        TarUnpacker::<xz2::read::XzDecoder<std::fs::File>>::new_gz
    );
}
