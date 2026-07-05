# starbase_archive

![Crates.io](https://img.shields.io/crates/v/starbase_archive)
![Crates.io](https://img.shields.io/crates/d/starbase_archive)

Abstractions and utilities for working with multiple archive formats. Currently supports `.tar`
(with gz, bz2, xz, zstd), `.zip`, and single-file codecs (`.gz`, `.zst`).

Formats and compression codecs are separate layers that compose over read/write streams:

- A **codec** (`Gz`, `Bz2`, `Xz`, `Zstd`) transforms bytes and wraps any `Read` or `Write` stream.
  The first read commits it to decompressing, the first write to compressing.
- A **format** (`TarPacker`/`TarUnpacker`, `ZipPacker`/`ZipUnpacker`, `FilePacker`/`FileUnpacker`)
  structures files and consumes or produces a stream.

```rust
use starbase_archive::Archiver;
use starbase_archive::codecs::Gz;
use starbase_archive::tar::{TarPacker, TarUnpacker};
use starbase_utils::fs;

let root = std::env::current_dir().unwrap();
let archive_file = root.join("output.tar.gz");

let mut archiver = Archiver::new(&root, &archive_file);
archiver.add_source_file("src/file.txt", None);
archiver.add_source_glob("docs/**/*.md");

// Pack and unpack based on the archive file extension
archiver.pack_from_ext()?;
archiver.unpack_from_ext()?;

// Or compose a format over a codec explicitly
archiver.pack(|file| Ok(TarPacker::new(Gz::new(fs::create_file(file)?))))?;
archiver.unpack(|dir, file| Ok(TarUnpacker::new(dir, Gz::new(fs::open_file(file)?))))?;
```

Zip is the exception: compression is part of the zip format itself and applied per entry, so it's
configured as an option instead of a codec.

```rust
use starbase_archive::zip::ZipPacker;
use zip::CompressionMethod;

archiver.pack(|file| {
    Ok(ZipPacker::with_compression(
        fs::create_file(file)?,
        CompressionMethod::Deflated,
    ))
})?;
```

When packing, writers must implement the `Finish` trait, which cascades trailing bytes
(compression epilogues, format footers) through the entire stream chain. It's implemented for
`File`, `Vec<u8>`, `Cursor`, `BufWriter`, and all codecs.
