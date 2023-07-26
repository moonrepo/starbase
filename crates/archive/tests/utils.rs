#[macro_export]
macro_rules! generate_tests {
    ($filename:expr, $packer:expr, $unpacker:expr) => {
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

            dbg!("size", archive.metadata().unwrap().len());

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

        // #[test]
        // fn file_via_glob() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["file.*"], &archive, None).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("file.txt").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("file.txt"),
        //         &output.join("file.txt")
        //     ));
        // }

        // #[test]
        // fn file_with_prefix() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(
        //         input,
        //         &string_vec!["file.txt"],
        //         &archive,
        //         Some("some/prefix"),
        //     )
        //     .unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("some/prefix/file.txt").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("file.txt"),
        //         &output.join("some/prefix/file.txt")
        //     ));
        // }

        // #[test]
        // fn file_via_glob_with_prefix() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["file.*"], &archive, Some("some/prefix")).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("some/prefix/file.txt").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("file.txt"),
        //         &output.join("some/prefix/file.txt")
        //     ));
        // }

        // #[test]
        // fn file_with_prefix_thats_removed() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(
        //         input,
        //         &string_vec!["file.txt"],
        //         &archive,
        //         Some("some/prefix"),
        //     )
        //     .unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, Some("some/prefix")).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("file.txt").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("file.txt"),
        //         &output.join("file.txt")
        //     ));
        // }

        // #[test]
        // fn nested_file_and_preserves_path() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(
        //         input,
        //         &string_vec!["folder/nested/other.js"],
        //         &archive,
        //         None,
        //     )
        //     .unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn file_and_dir_explicitly() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(
        //         input,
        //         &string_vec!["folder/nested", "file.txt"],
        //         &archive,
        //         None,
        //     )
        //     .unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("file.txt").exists());
        //     assert!(output.join("folder/nested/other.js").exists());
        //     assert!(!output.join("folder/file.js").exists()); // Should not exist!

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("file.txt"),
        //         &output.join("file.txt")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn dir() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["folder"], &archive, None).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("folder/file.js").exists());
        //     assert!(output.join("folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/file.js"),
        //         &output.join("folder/file.js")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn dir_via_glob() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["folder/**/*.js"], &archive, None).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("folder/file.js").exists());
        //     assert!(output.join("folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/file.js"),
        //         &output.join("folder/file.js")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn dir_with_prefix() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["folder"], &archive, Some("some/prefix")).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("some/prefix/folder/file.js").exists());
        //     assert!(output.join("some/prefix/folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/file.js"),
        //         &output.join("some/prefix/folder/file.js")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("some/prefix/folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn dir_via_glob_with_prefix() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(
        //         input,
        //         &string_vec!["folder/**/*.js"],
        //         &archive,
        //         Some("some/prefix"),
        //     )
        //     .unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, None).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("some/prefix/folder/file.js").exists());
        //     assert!(output.join("some/prefix/folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/file.js"),
        //         &output.join("some/prefix/folder/file.js")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("some/prefix/folder/nested/other.js")
        //     ));
        // }

        // #[test]
        // fn dir_with_prefix_thats_removed() {
        //     let sandbox = create_sandbox("archives");

        //     // Pack
        //     let input = sandbox.path();
        //     let archive = sandbox.path().join("out.$ext");

        //     tar(input, &string_vec!["folder"], &archive, Some("some/prefix")).unwrap();

        //     assert!(archive.exists());
        //     assert_ne!(archive.metadata().unwrap().len(), 0);

        //     // Unpack
        //     let output = sandbox.path().join("out");

        //     untar(&archive, &output, Some("some/prefix")).unwrap();

        //     assert!(output.exists());
        //     assert!(output.join("folder/file.js").exists());
        //     assert!(output.join("folder/nested/other.js").exists());

        //     // Compare
        //     assert!(file_contents_match(
        //         &input.join("folder/file.js"),
        //         &output.join("folder/file.js")
        //     ));
        //     assert!(file_contents_match(
        //         &input.join("folder/nested/other.js"),
        //         &output.join("folder/nested/other.js")
        //     ));
        // }
    };
}
