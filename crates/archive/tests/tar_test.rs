mod utils;

use starbase_archive::Archiver;
use starbase_archive::tar::*;
use starbase_sandbox::{create_empty_sandbox, create_sandbox};
use std::fs::File;
use std::path::Path;

// This can be used to create a tar file with a single
// entry that could be malicious (e.g. with entry "../leak.txt"
// that on unsecure extraction, could cause relative directory
// path traversal outside the destination for extraction)
fn create_malicious_tar_common<W: std::io::Write>(
    archive_writer: W,
    entry_path: &Path,
    entry_content: &[u8],
) {
    let mut builder = binstall_tar::Builder::new(archive_writer);
    let mut header = binstall_tar::Header::new_gnu();

    // Setting entry path manually (instead of using Header::set_path() as
    // that method doesn't allow setting malicious paths e.g.
    // ../../leak.txt that can help in testing zip slip attacks)
    header.set_size(entry_content.len() as u64);

    let entry_path_bytes = entry_path.to_str().unwrap().as_bytes();
    let header_bytes = header.as_mut_bytes();

    const GNU_TAR_NAME_LIMIT: usize = 100;
    let len = entry_path_bytes.len().min(GNU_TAR_NAME_LIMIT);
    header_bytes[..len].copy_from_slice(&entry_path_bytes[..len]);
    header.set_cksum();

    // Add content against the entry
    builder.append(&header, entry_content).unwrap();
    builder.finish().unwrap();
}

mod tar {
    use super::*;

    generate_tests!("out.tar", TarPacker::new, TarUnpacker::new);

    fn create_malicious_tar_plain(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        create_malicious_tar_common(file, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar", create_malicious_tar_plain);
}

mod tar_gz {
    use super::*;

    generate_tests!("out.tar.gz", TarPacker::new_gz, TarUnpacker::new_gz);

    fn create_malicious_tar_gz(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.gz", create_malicious_tar_gz);
}

mod tar_xz {
    use super::*;

    generate_tests!("out.tar.xz", TarPacker::new_xz, TarUnpacker::new_xz);

    fn create_malicious_tar_xz(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();

        // 6 suggested as a good default by the XzEncoderd doc
        const COMPRESSION_LEVEL: u32 = 6;
        let encoder = liblzma::write::XzEncoder::new(file, COMPRESSION_LEVEL);

        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.xz", create_malicious_tar_xz);
}

mod tar_zstd {
    use super::*;

    generate_tests!("out.tar.zst", TarPacker::new_zstd, TarUnpacker::new_zstd);

    fn create_malicious_tar_zstd(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();

        const ZSTD_DEFAULT_COMPRESSION_LEVEL: i32 = 3;
        let encoder = zstd::stream::Encoder::new(file, ZSTD_DEFAULT_COMPRESSION_LEVEL)
            .unwrap()
            .auto_finish();

        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.zst", create_malicious_tar_zstd);
}

mod tar_bz2 {
    use super::*;

    generate_tests!("out.tar.bz2", TarPacker::new_bz2, TarUnpacker::new_bz2);

    fn create_malicious_tar_bz2(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());

        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.bz2", create_malicious_tar_bz2);
}
