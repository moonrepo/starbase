#[macro_export]
macro_rules! generate_tests {
    ($filename:expr_2021, $packer:expr_2021, $unpacker:expr_2021) => {
        fn file_contents_match(a: &Path, b: &Path) -> bool {
            std::fs::read_to_string(a).unwrap() == std::fs::read_to_string(b).unwrap()
        }

        #[test]
        fn file() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("file.txt", None);
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("file.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("file.txt")
            ));
        }

        #[test]
        fn file_via_glob() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_glob("file.*");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("file.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("file.txt")
            ));
        }

        #[test]
        fn file_with_prefix() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("file.txt", None);
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("some/prefix/file.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("some/prefix/file.txt")
            ));
        }

        #[test]
        fn file_via_glob_with_prefix() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_glob("file.*");
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("some/prefix/file.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("some/prefix/file.txt")
            ));
        }

        #[test]
        fn file_with_prefix_thats_removed_when_unpacked() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("file.txt", None);
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let mut archiver = Archiver::new(&output, &archive);
            archiver.set_prefix("some/prefix");
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("file.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("file.txt")
            ));
        }

        #[test]
        fn nested_file_and_preserves_path() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("folder/nested/other.txt", None);
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }

        #[test]
        fn file_and_dir_explicitly() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("folder/nested", None);
            archiver.add_source_file("file.txt", None);
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("file.txt").exists());
            assert!(output.join("folder/nested/other.txt").exists());
            assert!(!output.join("folder/nested.txt").exists()); // Should not exist!

            // Compare
            assert!(file_contents_match(
                &input.join("file.txt"),
                &output.join("file.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("folder", None);
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("folder/nested.txt").exists());
            assert!(output.join("folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested.txt"),
                &output.join("folder/nested.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir_via_glob() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_glob("folder/**/*.txt");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("folder/nested.txt").exists());
            assert!(output.join("folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested.txt"),
                &output.join("folder/nested.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir_with_prefix() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("folder", None);
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("some/prefix/folder/nested.txt").exists());
            assert!(output.join("some/prefix/folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested.txt"),
                &output.join("some/prefix/folder/nested.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("some/prefix/folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir_via_glob_with_prefix() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_glob("folder/**/*.txt");
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("some/prefix/folder/nested.txt").exists());
            assert!(output.join("some/prefix/folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested.txt"),
                &output.join("some/prefix/folder/nested.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("some/prefix/folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir_with_prefix_thats_removed_when_unpacked() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_file("folder", None);
            archiver.set_prefix("some/prefix");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let mut archiver = Archiver::new(&output, &archive);
            archiver.set_prefix("some/prefix");
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(output.join("folder/nested.txt").exists());
            assert!(output.join("folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested.txt"),
                &output.join("folder/nested.txt")
            ));
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }

        #[test]
        fn dir_with_negated_glob() {
            let sandbox = create_sandbox("archives");

            // Pack
            let input = sandbox.path();
            let archive = sandbox.path().join($filename);

            let mut archiver = Archiver::new(input, &archive);
            archiver.add_source_glob("folder/**/*.txt");
            archiver.add_source_glob("!folder/nested.txt");
            archiver.pack($packer).unwrap();

            assert!(archive.exists());
            assert_ne!(archive.metadata().unwrap().len(), 0);

            // Unpack
            let output = sandbox.path().join("out");

            let archiver = Archiver::new(&output, &archive);
            archiver.unpack($unpacker).unwrap();

            assert!(output.exists());
            assert!(!output.join("file.txt").exists());
            assert!(!output.join("folder/nested.txt").exists());
            assert!(output.join("folder/nested/other.txt").exists());

            // Compare
            assert!(file_contents_match(
                &input.join("folder/nested/other.txt"),
                &output.join("folder/nested/other.txt")
            ));
        }
    };
}
