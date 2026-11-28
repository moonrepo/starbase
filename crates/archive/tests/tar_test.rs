mod utils;

use starbase_archive::codecs::*;
use starbase_archive::tar::*;
use starbase_archive::{ArchiveUnpacker, Archiver};
use starbase_sandbox::{create_empty_sandbox, create_sandbox};
use starbase_utils::fs;
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
    // The builder writes paths verbatim, including malicious
    // ones like ../../leak.txt (for testing zip slip attacks)
    let mut builder = jdx_tar::Builder::new(archive_writer);
    let mut header = jdx_tar::Header::new_gnu(jdx_tar::EntryType::File);
    header.set_mode(0o644);
    header.set_size(entry_content.len() as u64);

    builder
        .append_data(&mut header, entry_path, entry_content)
        .unwrap();
    builder.finish().unwrap();
}

// Writes raw tar blocks, for archives that `jdx_tar::Builder` won't
// write, like the variants produced by other tools.
mod raw {
    // POSIX ustar
    pub const USTAR: &[u8; 8] = b"ustar\x0000";
    // GNU tar
    pub const GNU: &[u8; 8] = b"ustar  \0";
    // Maven's assembly plugin (plexus-archiver), which publishes many
    // archives on Maven Central, like JRuby's
    pub const MAVEN: &[u8; 8] = b"ustar\0\0\0";

    fn octal(field: &mut [u8], value: u64) {
        let width = field.len() - 1;
        field.copy_from_slice(format!("{value:0width$o}\0").as_bytes());
    }

    pub fn header(identity: &[u8; 8], typeflag: u8, name: &str, size: u64) -> [u8; 512] {
        assert!(name.len() <= 100, "name too long for the header");

        let mut block = [0; 512];
        block[..name.len()].copy_from_slice(name.as_bytes());
        octal(&mut block[100..108], 0o644);
        octal(&mut block[108..116], 0);
        octal(&mut block[116..124], 0);
        octal(&mut block[124..136], size);
        octal(&mut block[136..148], 0);
        block[156] = typeflag;
        block[257..265].copy_from_slice(identity);
        block
    }

    pub fn set_mode(block: &mut [u8; 512], mode: u32) {
        octal(&mut block[100..108], u64::from(mode));
    }

    pub fn set_link(block: &mut [u8; 512], target: &str) {
        block[157..157 + target.len()].copy_from_slice(target.as_bytes());
    }

    pub fn set_prefix(block: &mut [u8; 512], prefix: &str) {
        block[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    }

    // Old GNU sparse headers hold up to 4 (offset, length) extents
    pub fn set_gnu_sparse(block: &mut [u8; 512], extents: &[(u64, u64)], real_size: u64) {
        for (index, (offset, len)) in extents.iter().enumerate() {
            let start = 386 + index * 24;
            octal(&mut block[start..start + 12], *offset);
            octal(&mut block[start + 12..start + 24], *len);
        }

        octal(&mut block[483..495], real_size);
    }

    fn pax_record(key: &str, value: &str) -> String {
        // The length prefix counts its own digits
        let base = key.len() + value.len() + 3;
        let mut len = base + 1;

        while len != base + len.to_string().len() {
            len = base + len.to_string().len();
        }

        format!("{len} {key}={value}\n")
    }

    #[derive(Default)]
    pub struct Tar(Vec<u8>);

    impl Tar {
        pub fn push(&mut self, mut block: [u8; 512], data: &[u8]) -> &mut Self {
            block[148..156].fill(b' ');
            let sum: u32 = block.iter().map(|byte| u32::from(*byte)).sum();
            block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());

            self.0.extend_from_slice(&block);
            self.0.extend_from_slice(data);
            self.0
                .resize(self.0.len() + (512 - data.len() % 512) % 512, 0);
            self
        }

        // GNU long name (`L`) or long link (`K`) record
        pub fn long(&mut self, identity: &[u8; 8], typeflag: u8, value: &str) -> &mut Self {
            let mut data = value.as_bytes().to_vec();
            data.push(0);
            self.push(
                header(identity, typeflag, "././@LongLink", data.len() as u64),
                &data,
            )
        }

        pub fn pax(&mut self, identity: &[u8; 8], records: &[(&str, &str)]) -> &mut Self {
            let data = records
                .iter()
                .map(|(key, value)| pax_record(key, value))
                .collect::<String>();

            self.push(
                header(identity, b'x', "././@PaxHeader", data.len() as u64),
                data.as_bytes(),
            )
        }

        pub fn file(&mut self, identity: &[u8; 8], name: &str, data: &[u8]) -> &mut Self {
            self.push(header(identity, b'0', name, data.len() as u64), data)
        }

        pub fn write(&mut self, path: &std::path::Path) {
            self.0.extend_from_slice(&[0; 1024]);
            std::fs::write(path, &self.0).unwrap();
        }
    }
}

// Unpack a raw `.tar` with the prefix stripped, like proto does.
fn unpack_raw(tar: &mut raw::Tar, prefix: &str) -> (starbase_sandbox::Sandbox, Result<(), String>) {
    let sandbox = create_empty_sandbox();
    let archive = sandbox.path().join("raw.tar");
    tar.write(&archive);

    let result = TarUnpacker::new(&sandbox.path().join("out"), File::open(&archive).unwrap())
        .unpack(prefix)
        .map(|_| ())
        .map_err(|error| format!("{error:?}"));

    (sandbox, result)
}

mod compat {
    use super::raw::*;
    use super::*;

    const LONG: &str = "tool-1.0.0/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/hello.txt";
    const LONG_UNPREFIXED: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/hello.txt";

    fn truncated(path: &str) -> &str {
        &path[..100]
    }

    fn long_name_tar(identity: &[u8; 8]) -> Tar {
        let mut tar = Tar::default();
        tar.long(identity, b'L', LONG)
            .file(identity, truncated(LONG), b"hello\n");
        tar
    }

    #[test]
    fn unpacks_gnu_long_names() {
        let (sandbox, result) = unpack_raw(&mut long_name_tar(GNU), "tool-1.0.0");

        result.unwrap();
        assert_eq!(
            std::fs::read(sandbox.path().join("out").join(LONG_UNPREFIXED)).unwrap(),
            b"hello\n"
        );
    }

    // https://github.com/moonrepo/proto/issues/1159
    #[test]
    fn never_truncates_long_names_with_maven_headers() {
        let (sandbox, result) = unpack_raw(&mut long_name_tar(MAVEN), "tool-1.0.0");
        let out = sandbox.path().join("out");

        assert!(
            !out.join(truncated(LONG_UNPREFIXED)).exists(),
            "extracted to the truncated 100 byte name"
        );

        if result.is_ok() {
            assert!(out.join(LONG_UNPREFIXED).exists());
        }
    }

    // https://github.com/moonrepo/proto/issues/1159
    #[test]
    fn unpacks_long_names_with_maven_headers() {
        let dir = format!("tool-1.0.0/{}/MethodNotAllowedResponse/", "x".repeat(80));
        let file = format!("{dir}cdesc-MethodNotAllowedResponse.ri");
        // The 100th byte of the path is a slash
        let slash = format!("tool-1.0.0/{}/deep/file.txt", "c".repeat(88));

        let mut dir_block = header(MAVEN, b'5', truncated(&dir), 0);
        set_mode(&mut dir_block, 0o755);

        let mut tar = long_name_tar(MAVEN);
        tar.long(MAVEN, b'L', &dir)
            .push(dir_block, &[])
            .long(MAVEN, b'L', &file)
            .file(MAVEN, truncated(&file), b"ri\n")
            .long(MAVEN, b'L', &slash)
            .file(MAVEN, truncated(&slash), b"slash\n");

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");
        let out = sandbox.path().join("out");

        result.unwrap();
        assert_eq!(
            std::fs::read(out.join(LONG_UNPREFIXED)).unwrap(),
            b"hello\n"
        );
        assert_eq!(
            std::fs::read(out.join(file.strip_prefix("tool-1.0.0/").unwrap())).unwrap(),
            b"ri\n"
        );
        assert_eq!(
            std::fs::read(out.join(slash.strip_prefix("tool-1.0.0/").unwrap())).unwrap(),
            b"slash\n"
        );
    }

    #[test]
    fn unpacks_long_link_targets_with_maven_headers() {
        let mut link = header(MAVEN, b'2', "tool-1.0.0/link", 0);
        set_link(&mut link, truncated(LONG_UNPREFIXED));

        let mut tar = long_name_tar(MAVEN);
        tar.long(MAVEN, b'K', LONG_UNPREFIXED).push(link, &[]);

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert_eq!(
            std::fs::read_link(sandbox.path().join("out/link")).unwrap(),
            Path::new(LONG_UNPREFIXED)
        );
    }

    #[test]
    fn unpacks_prefixed_paths_with_maven_headers() {
        let (prefix, name) = LONG.rsplit_once('/').unwrap();
        let (prefix, dir) = prefix.rsplit_once('/').unwrap();
        let name = format!("{dir}/{name}");

        let mut block = header(MAVEN, b'0', &name, 6);
        set_prefix(&mut block, prefix);

        let mut tar = Tar::default();
        tar.push(block, b"hello\n");

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert!(sandbox.path().join("out").join(LONG_UNPREFIXED).exists());
    }

    #[test]
    fn unpacks_pax_paths_with_maven_headers() {
        let mut tar = Tar::default();
        tar.pax(MAVEN, &[("path", LONG)])
            .file(MAVEN, truncated(LONG), b"hello\n");

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert!(sandbox.path().join("out").join(LONG_UNPREFIXED).exists());
    }

    const HEAD: &[u8] = b"head";
    const TAIL: &[u8] = b"tail";
    const TAIL_OFFSET: u64 = 1024 * 1024;

    fn sparse_contents() -> Vec<u8> {
        let mut contents = vec![0; TAIL_OFFSET as usize + TAIL.len()];
        contents[..HEAD.len()].copy_from_slice(HEAD);
        contents[TAIL_OFFSET as usize..].copy_from_slice(TAIL);
        contents
    }

    // https://github.com/moonrepo/starbase/issues/56
    #[test]
    fn unpacks_pax_sparse_files() {
        let real_size = sparse_contents().len().to_string();
        let map = format!("2\n0\n{}\n{TAIL_OFFSET}\n{}\n", HEAD.len(), TAIL.len());

        let mut data = map.into_bytes();
        data.resize(512, 0);
        data.extend_from_slice(HEAD);
        data.extend_from_slice(TAIL);

        let mut tar = Tar::default();
        tar.pax(
            USTAR,
            &[
                ("GNU.sparse.major", "1"),
                ("GNU.sparse.minor", "0"),
                ("GNU.sparse.name", "tool-1.0.0/sparse.bin"),
                ("GNU.sparse.realsize", &real_size),
            ],
        )
        .file(USTAR, "tool-1.0.0/GNUSparseFile.0/sparse.bin", &data);

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");
        let out = sandbox.path().join("out");

        result.unwrap();
        assert_eq!(
            std::fs::read(out.join("sparse.bin")).unwrap(),
            sparse_contents()
        );
        assert!(!out.join("GNUSparseFile.0").exists());
    }

    #[test]
    fn unpacks_gnu_sparse_files() {
        let mut block = header(GNU, b'S', "tool-1.0.0/sparse.bin", 8);
        set_gnu_sparse(
            &mut block,
            &[(0, HEAD.len() as u64), (TAIL_OFFSET, TAIL.len() as u64)],
            sparse_contents().len() as u64,
        );

        let mut tar = Tar::default();
        tar.push(block, &[HEAD, TAIL].concat());

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert_eq!(
            std::fs::read(sandbox.path().join("out/sparse.bin")).unwrap(),
            sparse_contents()
        );
    }

    #[test]
    fn unpacks_hard_links_within_the_prefix() {
        let mut link = header(GNU, b'1', "tool-1.0.0/bin/link", 0);
        set_link(&mut link, "tool-1.0.0/bin/real");

        let mut tar = Tar::default();
        tar.file(GNU, "tool-1.0.0/bin/real", b"real\n")
            .push(link, &[]);

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert_eq!(
            std::fs::read(sandbox.path().join("out/bin/link")).unwrap(),
            b"real\n"
        );
    }

    #[test]
    fn keeps_entries_outside_the_prefix() {
        let mut tar = Tar::default();
        tar.file(GNU, "tool-1.0.0/inside.txt", b"inside\n").file(
            GNU,
            "other/outside.txt",
            b"outside\n",
        );

        let (sandbox, result) = unpack_raw(&mut tar, "tool-*");
        let out = sandbox.path().join("out");

        result.unwrap();
        assert!(out.join("inside.txt").exists());
        assert!(out.join("other/outside.txt").exists());
    }

    #[test]
    fn resolves_absolute_paths() {
        let mut tar = Tar::default();
        tar.file(GNU, "/tool-1.0.0/absolute.txt", b"absolute\n");

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");

        result.unwrap();
        assert!(sandbox.path().join("out/absolute.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn restores_permissions_without_special_bits() {
        use std::os::unix::fs::PermissionsExt;

        let mut exe = header(GNU, b'0', "tool-1.0.0/bin/exe", 4);
        set_mode(&mut exe, 0o755);

        let mut setuid = header(GNU, b'0', "tool-1.0.0/bin/setuid", 4);
        set_mode(&mut setuid, 0o4755);

        let mut tar = Tar::default();
        tar.push(exe, b"exe\n").push(setuid, b"uid\n");

        let (sandbox, result) = unpack_raw(&mut tar, "tool-1.0.0");
        let mode = |name: &str| {
            std::fs::metadata(sandbox.path().join("out/bin").join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777
        };

        result.unwrap();
        assert_eq!(mode("exe"), 0o755);
        assert_eq!(mode("setuid"), 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn unpacks_into_a_symlinked_output_dir() {
        let mut tar = Tar::default();
        tar.file(GNU, "tool-1.0.0/file.txt", b"file\n");

        let sandbox = create_empty_sandbox();
        let archive = sandbox.path().join("raw.tar");
        let real = sandbox.path().join("real");
        let out = sandbox.path().join("out");
        tar.write(&archive);

        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &out).unwrap();

        TarUnpacker::new(&out, File::open(&archive).unwrap())
            .unpack("tool-1.0.0")
            .unwrap();

        assert!(real.join("file.txt").exists());
    }
}

mod tar {
    use super::*;

    generate_tests!(
        "out.tar",
        |file| Ok(TarPacker::new(fs::create_file(file)?)),
        |dir, file| Ok(TarUnpacker::new(dir, fs::open_file(file)?))
    );

    fn create_malicious_tar_plain(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        create_malicious_tar_common(file, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar", create_malicious_tar_plain);
}

mod tar_gz {
    use super::*;

    generate_tests!(
        "out.tar.gz",
        |file| Ok(TarPacker::new(Gz::new(fs::create_file(file)?))),
        |dir, file| Ok(TarUnpacker::new(dir, Gz::new(fs::open_file(file)?)))
    );

    fn create_malicious_tar_gz(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.gz", create_malicious_tar_gz);
}

mod tar_xz {
    use super::*;

    generate_tests!(
        "out.tar.xz",
        |file| Ok(TarPacker::new(Xz::new(fs::create_file(file)?))),
        |dir, file| Ok(TarUnpacker::new(dir, Xz::new(fs::open_file(file)?)))
    );

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

    generate_tests!(
        "out.tar.zst",
        |file| Ok(TarPacker::new(Zstd::new(fs::create_file(file)?))),
        |dir, file| Ok(TarUnpacker::new(dir, Zstd::new(fs::open_file(file)?)))
    );

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

    generate_tests!(
        "out.tar.bz2",
        |file| Ok(TarPacker::new(Bz2::new(fs::create_file(file)?))),
        |dir, file| Ok(TarUnpacker::new(dir, Bz2::new(fs::open_file(file)?)))
    );

    fn create_malicious_tar_bz2(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());

        create_malicious_tar_common(encoder, entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.bz2", create_malicious_tar_bz2);
}

mod tar_z {
    use super::*;

    generate_tests!(
        "out.tar.Z",
        |file| Ok(TarPacker::new(Z::new(fs::create_file(file)?))),
        |dir, file| Ok(TarUnpacker::new(dir, Z::new(fs::open_file(file)?)))
    );

    fn create_malicious_tar_z(archive_path: &Path, entry_path: &Path, entry_content: &[u8]) {
        let file = File::create(archive_path).unwrap();

        // The encoder writes the final code and buffered bytes on drop
        create_malicious_tar_common(Z::new(file), entry_path, entry_content);
    }

    generate_relative_path_traversal_tests!("malicious.tar.Z", create_malicious_tar_z);

    #[test]
    fn unpacks_archive_created_by_system_compress() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/__fixtures__/compress/contents.tar.Z");
        let sandbox = create_empty_sandbox();
        let output = sandbox.path().join("out");

        let archiver = Archiver::new(&output, &fixture);
        let (ext, _) = archiver.unpack_from_ext().unwrap();

        assert_eq!(ext, "tar.Z");
        assert!(output.join("file.txt").exists());
        assert!(output.join("folder/nested.txt").exists());
        assert!(output.join("folder/nested/other.txt").exists());

        // Compare against a literal: the fixture tarball holds the
        // original LF content, while git checks `file.txt` out with
        // CRLF line endings on Windows
        assert_eq!(std::fs::read(output.join("file.txt")).unwrap(), b"file\n");
    }

    #[test]
    fn unpacks_via_aliased_extensions() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/__fixtures__/compress/contents.tar.Z");

        for alias in ["tar.z", "taZ", "tZ"] {
            let sandbox = create_empty_sandbox();
            let archive = sandbox.path().join(format!("bundle.{alias}"));
            let output = sandbox.path().join("out");

            std::fs::copy(&fixture, &archive).unwrap();

            let archiver = Archiver::new(&output, &archive);
            let (ext, _) = archiver.unpack_from_ext().unwrap();

            assert_eq!(ext, alias);
            assert!(
                output.join("file.txt").exists(),
                "expected file.txt when unpacking .{alias}"
            );
        }
    }
}
