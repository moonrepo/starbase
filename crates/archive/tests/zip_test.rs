mod utils;

use ::zip::write::SimpleFileOptions;
use ::zip::{CompressionMethod, ZipWriter};
use starbase_archive::Archiver;
use starbase_archive::zip::*;
use starbase_sandbox::{create_empty_sandbox, create_sandbox};
use std::io::Write;
use std::path::Path;

// This can be used to create a zip file with a single
// entry that could be malicious (e.g. with entry "../leak.txt"
// that on unsecure extraction, could cause relative directory
// path traversal outside the destination for extraction)
fn create_malicious_zip_common(
    archive_path: &Path,
    entry_path: &Path,
    entry_content: &[u8],
    method: CompressionMethod,
) {
    let file = std::fs::File::create(archive_path).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(method);
    let entry_path_str = entry_path.to_str().unwrap();

    writer.start_file(entry_path_str, options).unwrap();
    writer.write_all(entry_content).unwrap();
    writer.finish().unwrap();
}

mod zip {
    use super::*;

    generate_tests!("out.zip", ZipPacker::new, ZipUnpacker::new);

    fn create_malicious_zip_plain(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        create_malicious_zip_common(
            archive_path,
            entry_path,
            entry_content,
            CompressionMethod::Stored,
        );
    }

    generate_relative_path_traversal_test!("malicious.zip", create_malicious_zip_plain);
}

mod zip_deflate {
    use super::*;

    generate_tests!("out.zip", ZipPacker::new_deflate, ZipUnpacker::new_deflate);

    fn create_malicious_zip_deflated(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        create_malicious_zip_common(
            archive_path,
            entry_path,
            entry_content,
            CompressionMethod::Deflated,
        );
    }

    generate_relative_path_traversal_test!("malicious.zip", create_malicious_zip_deflated);
}
