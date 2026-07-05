use crate::archive_error::ArchiveError;
use crate::{get_full_file_extension, join_file_name};
use rustc_hash::FxHashMap;
use starbase_utils::glob;
use std::path::{Path, PathBuf};
use tracing::{instrument, trace};

/// Abstraction for packing archives.
pub trait ArchivePacker {
    /// Add the source file to the archive.
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError>;

    /// Add the source directory to the archive.
    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError>;

    /// Finalize the archive, writing any trailing data (compression
    /// epilogues, format footers) through the entire stream chain.
    fn pack(self) -> Result<(), ArchiveError>
    where
        Self: Sized;
}

/// Abstraction for unpacking archives.
pub trait ArchiveUnpacker {
    /// Unpack the archive to the destination directory. If a prefix is provided,
    /// remove it from the start of all file paths within the archive.
    fn unpack(self, prefix: &str) -> Result<PathBuf, ArchiveError>
    where
        Self: Sized;
}

/// An `Archiver` is an abstraction for packing and unpacking archives.
/// When packing, the added source files and globs are included in the
/// archive, relative from the source root.
#[derive(Debug)]
pub struct Archiver<'owner> {
    /// The archive file itself (`.zip`, etc).
    archive_file: &'owner Path,

    /// Prefix to append to all files.
    prefix: &'owner str,

    /// Absolute file path to source, to relative file path in archive.
    source_files: FxHashMap<PathBuf, String>,

    /// Globs to find files with.
    source_globs: Vec<String>,

    /// For packing, the root to join source files with.
    /// For unpacking, the root to extract files relative to.
    pub source_root: &'owner Path,
}

impl<'owner> Archiver<'owner> {
    /// Create a new archiver.
    pub fn new(source_root: &'owner Path, archive_file: &'owner Path) -> Self {
        Archiver {
            archive_file,
            prefix: "",
            source_files: FxHashMap::default(),
            source_globs: vec![],
            source_root,
        }
    }

    /// Add a source file to be included in the archive when packing.
    /// The file path can be relative from the source root, or absolute.
    /// A custom file path can be used within the archive, otherwise the
    /// file will be placed relative from the source root.
    pub fn add_source_file<F: AsRef<Path>>(
        &mut self,
        source: F,
        custom_name: Option<&str>,
    ) -> &mut Self {
        let source = source.as_ref();
        let source = source.strip_prefix(self.source_root).unwrap_or(source);

        self.source_files.insert(
            self.source_root.join(source),
            custom_name
                .map(|n| n.to_owned())
                .unwrap_or_else(|| source.to_string_lossy().to_string()),
        );

        self
    }

    /// Add a glob that'll find files, relative from the source root,
    /// to be included in the archive when packing.
    pub fn add_source_glob<G: AsRef<str>>(&mut self, glob: G) -> &mut Self {
        let glob = glob.as_ref().to_owned();

        if !self.source_globs.contains(&glob) {
            self.source_globs.push(glob);
        }

        self
    }

    /// Set the prefix to prepend to files with when packing,
    /// and to remove when unpacking.
    pub fn set_prefix(&mut self, prefix: &'owner str) -> &mut Self {
        self.prefix = prefix;
        self
    }

    /// Pack and create the archive with the added source, using the
    /// provided packer factory. The factory is passed an absolute
    /// path to the destination archive file, which is also returned
    /// from this method.
    #[instrument(skip_all)]
    pub fn pack<F, P>(&self, packer: F) -> Result<PathBuf, ArchiveError>
    where
        F: FnOnce(&Path) -> Result<P, ArchiveError>,
        P: ArchivePacker,
    {
        trace!(
            input_dir = ?self.source_root,
            output_file = ?self.archive_file,
            "Packing archive",
        );

        let mut archive = packer(self.archive_file)?;

        for (source, file) in &self.source_files {
            if !source.exists() {
                trace!(source = ?source, "Source file does not exist, skipping");

                continue;
            }

            let name = join_file_name([self.prefix, file]);

            if source.is_file() {
                archive.add_file(&name, source)?;
            } else {
                archive.add_dir(&name, source)?;
            }
        }

        if !self.source_globs.is_empty() {
            trace!(globs = ?self.source_globs, "Packing files using glob");

            for file in glob::walk_files(self.source_root, &self.source_globs)? {
                let file_name = file
                    .strip_prefix(self.source_root)
                    .unwrap()
                    .to_str()
                    .unwrap();

                archive.add_file(&join_file_name([self.prefix, file_name]), &file)?;
            }
        }

        archive.pack()?;

        Ok(self.archive_file.to_path_buf())
    }

    /// Determine the packer to use based on the archive file extension,
    /// then pack the archive using [`Archiver#pack`].
    pub fn pack_from_ext(&self) -> Result<(String, PathBuf), ArchiveError> {
        let ext = get_full_file_extension(self.archive_file);

        macro_rules! pack {
            ($feature:literal, $enabled:meta, $factory:expr) => {{
                #[cfg($enabled)]
                {
                    self.pack($factory)
                }

                #[cfg(not($enabled))]
                Err(ArchiveError::FeatureNotEnabled {
                    feature: $feature.into(),
                    path: self.archive_file.to_path_buf(),
                })
            }};
        }

        let out = match ext.as_deref() {
            Some("gz" | "gzip") => pack!("gz", feature = "gz", |file| {
                Ok(crate::file::FilePacker::new(crate::codecs::Gz::new(
                    starbase_utils::fs::create_file(file)?,
                )))
            }),
            Some("tar") => pack!("tar", feature = "tar", |file| {
                Ok(crate::tar::TarPacker::new(starbase_utils::fs::create_file(
                    file,
                )?))
            }),
            Some("tar.bz2" | "tbz" | "tbz2" | "tz2") => {
                pack!("tar-bz2", all(feature = "tar", feature = "bz2"), |file| {
                    Ok(crate::tar::TarPacker::new(crate::codecs::Bz2::new(
                        starbase_utils::fs::create_file(file)?,
                    )))
                })
            }
            Some("tar.gz" | "tgz") => {
                pack!("tar-gz", all(feature = "tar", feature = "gz"), |file| {
                    Ok(crate::tar::TarPacker::new(crate::codecs::Gz::new(
                        starbase_utils::fs::create_file(file)?,
                    )))
                })
            }
            Some("tar.xz" | "txz") => {
                pack!("tar-xz", all(feature = "tar", feature = "xz"), |file| {
                    Ok(crate::tar::TarPacker::new(crate::codecs::Xz::new(
                        starbase_utils::fs::create_file(file)?,
                    )))
                })
            }
            Some("tar.zstd" | "tar.zst" | "tzst" | "tzs") => {
                pack!("tar-zstd", all(feature = "tar", feature = "zstd"), |file| {
                    Ok(crate::tar::TarPacker::new(crate::codecs::Zstd::new(
                        starbase_utils::fs::create_file(file)?,
                    )))
                })
            }
            Some("zst" | "zstd") => pack!("zstd", feature = "zstd", |file| {
                Ok(crate::file::FilePacker::new(crate::codecs::Zstd::new(
                    starbase_utils::fs::create_file(file)?,
                )))
            }),
            Some("zip") => pack!("zip", feature = "zip", |file| {
                Ok(crate::zip::ZipPacker::new(starbase_utils::fs::create_file(
                    file,
                )?))
            }),
            Some(ext) => Err(ArchiveError::UnsupportedFormat {
                format: ext.into(),
                path: self.archive_file.to_path_buf(),
            }),
            None => Err(ArchiveError::UnknownFormat {
                path: self.archive_file.to_path_buf(),
            }),
        }?;

        Ok((ext.unwrap(), out))
    }

    /// Unpack the archive to the destination root, using the provided
    /// unpacker factory. The factory is passed an absolute path
    /// to the output directory, and the input archive file. The unpacked
    /// directory or file is returned from this method.
    #[instrument(skip_all)]
    pub fn unpack<F, P>(&self, unpacker: F) -> Result<PathBuf, ArchiveError>
    where
        F: FnOnce(&Path, &Path) -> Result<P, ArchiveError>,
        P: ArchiveUnpacker,
    {
        trace!(
            output_dir = ?self.source_root,
            input_file = ?self.archive_file,
            "Unpacking archive",
        );

        let archive = unpacker(self.source_root, self.archive_file)?;

        archive.unpack(self.prefix)
    }

    /// Determine the unpacker to use based on the archive file extension,
    /// then unpack the archive using [`Archiver#unpack`].
    ///
    /// Returns an absolute path to the directory or file that was created,
    /// and the extension that was extracted from the input archive file.
    pub fn unpack_from_ext(&self) -> Result<(String, PathBuf), ArchiveError> {
        let ext = get_full_file_extension(self.archive_file);

        macro_rules! unpack {
            ($feature:literal, $enabled:meta, $factory:expr) => {{
                #[cfg($enabled)]
                {
                    self.unpack($factory)
                }

                #[cfg(not($enabled))]
                Err(ArchiveError::FeatureNotEnabled {
                    feature: $feature.into(),
                    path: self.archive_file.to_path_buf(),
                })
            }};
        }

        let out = match ext.as_deref() {
            Some("gz" | "gzip") => unpack!("gz", feature = "gz", |dir, file| {
                Ok(crate::file::FileUnpacker::new(
                    dir.join(crate::strip_compression_suffix(
                        &starbase_utils::fs::file_name(file),
                    )),
                    crate::codecs::Gz::new(starbase_utils::fs::open_file(file)?),
                ))
            }),
            Some("tar") => unpack!("tar", feature = "tar", |dir, file| {
                Ok(crate::tar::TarUnpacker::new(
                    dir,
                    starbase_utils::fs::open_file(file)?,
                ))
            }),
            Some("tar.bz2" | "tbz" | "tbz2" | "tz2") => unpack!(
                "tar-bz2",
                all(feature = "tar", feature = "bz2"),
                |dir, file| {
                    Ok(crate::tar::TarUnpacker::new(
                        dir,
                        crate::codecs::Bz2::new(starbase_utils::fs::open_file(file)?),
                    ))
                }
            ),
            Some("tar.gz" | "tgz") => unpack!(
                "tar-gz",
                all(feature = "tar", feature = "gz"),
                |dir, file| {
                    Ok(crate::tar::TarUnpacker::new(
                        dir,
                        crate::codecs::Gz::new(starbase_utils::fs::open_file(file)?),
                    ))
                }
            ),
            Some("tar.xz" | "txz") => unpack!(
                "tar-xz",
                all(feature = "tar", feature = "xz"),
                |dir, file| {
                    Ok(crate::tar::TarUnpacker::new(
                        dir,
                        crate::codecs::Xz::new(starbase_utils::fs::open_file(file)?),
                    ))
                }
            ),
            Some("tar.zstd" | "tar.zst" | "tzst" | "tzs") => unpack!(
                "tar-zstd",
                all(feature = "tar", feature = "zstd"),
                |dir, file| {
                    Ok(crate::tar::TarUnpacker::new(
                        dir,
                        crate::codecs::Zstd::new(starbase_utils::fs::open_file(file)?),
                    ))
                }
            ),
            Some("zst" | "zstd") => unpack!("zstd", feature = "zstd", |dir, file| {
                Ok(crate::file::FileUnpacker::new(
                    dir.join(crate::strip_compression_suffix(
                        &starbase_utils::fs::file_name(file),
                    )),
                    crate::codecs::Zstd::new(starbase_utils::fs::open_file(file)?),
                ))
            }),
            Some("zip") => unpack!("zip", feature = "zip", |dir, file| {
                crate::zip::ZipUnpacker::new(dir, starbase_utils::fs::open_file(file)?)
            }),
            Some(ext) => Err(ArchiveError::UnsupportedFormat {
                format: ext.into(),
                path: self.archive_file.to_path_buf(),
            }),
            None => Err(ArchiveError::UnknownFormat {
                path: self.archive_file.to_path_buf(),
            }),
        }?;

        Ok((ext.unwrap(), out))
    }
}
