use crate::archive_error::ArchiveError;
use crate::tree_differ::TreeDiffer;
use crate::{get_full_file_extension, join_file_name};
use rustc_hash::{FxHashMap, FxHashSet};
use starbase_utils::glob;
use std::path::{Path, PathBuf};
use tracing::trace;

#[cfg(not(feature = "miette"))]
pub type ArchiveResult<T> = Result<T, Box<dyn std::error::Error>>;

#[cfg(feature = "miette")]
pub type ArchiveResult<T> = miette::Result<T>;

/// Abstraction for packing archives.
pub trait ArchivePacker {
    /// Add the source file to the archive.
    fn add_file(&mut self, name: &str, file: &Path) -> ArchiveResult<()>;

    /// Add the source directory to the archive.
    fn add_dir(&mut self, name: &str, dir: &Path) -> ArchiveResult<()>;

    /// Create the archive and write all contents to disk.
    fn pack(&mut self) -> ArchiveResult<()>;
}

/// Abstraction for unpacking archives.
pub trait ArchiveUnpacker {
    /// Unpack the archive to the destination directory. If a prefix is provided,
    /// remove it from the start of all file paths within the archive.
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> ArchiveResult<PathBuf>;
}

/// An `Archiver` is an abstraction for packing and unpacking archives,
/// that utilizes the same set of sources for both operations. For packing,
/// the sources are the files that will be included in the archive. For unpacking,
/// the sources are used for file tree diffing when extracting the archive.
#[derive(Debug)]
pub struct Archiver<'owner> {
    /// The archive file itself (`.zip`, etc).
    archive_file: &'owner Path,

    /// Prefix to append to all files.
    prefix: &'owner str,

    /// Absolute file path to source, to relative file path in archive.
    source_files: FxHashMap<PathBuf, String>,

    /// Glob to finds files with.
    source_globs: FxHashSet<String>,

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
            source_globs: FxHashSet::default(),
            source_root,
        }
    }

    /// Add a source file to be used in the archiving process. The file path
    /// can be relative from the source root, or absolute. A custom file path
    /// can be used within the archive, otherwise the file will be placed
    /// relative from the source root.
    ///
    /// For packing, this includes the file in the archive.
    /// For unpacking, this diffs the file when extracting.
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

    /// Add a glob that'll find files, relative from the source root, to be
    /// used in the archiving process.
    ///
    /// For packing, this finds files to include in the archive.
    /// For unpacking, this finds files to diff against when extracting.
    pub fn add_source_glob<G: AsRef<str>>(&mut self, glob: G) -> &mut Self {
        self.source_globs.insert(glob.as_ref().to_owned());
        self
    }

    /// Set the prefix to prepend to files wth when packing,
    /// and to remove when unpacking.
    pub fn set_prefix(&mut self, prefix: &'owner str) -> &mut Self {
        self.prefix = prefix;
        self
    }

    /// Pack and create the archive with the added source, using the
    /// provided packer factory. The factory is passed an absolute
    /// path to the destination archive file, which is also returned
    /// from this method.
    pub fn pack<F, P>(&self, packer: F) -> ArchiveResult<PathBuf>
    where
        F: FnOnce(&Path) -> ArchiveResult<P>,
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
    /// then pack the archive using [`Archiver.pack`].
    pub fn pack_from_ext(&self) -> ArchiveResult<()> {
        match get_full_file_extension(self.archive_file).as_deref() {
            Some(".gz") => {
                #[cfg(feature = "gz")]
                self.pack(crate::gz::GzPacker::new)?;

                #[cfg(not(feature = "gz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "gz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar") => {
                #[cfg(feature = "tar")]
                self.pack(crate::tar::TarPacker::new)?;

                #[cfg(not(feature = "tar"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar.gz" | ".tgz") => {
                #[cfg(feature = "tar-gz")]
                self.pack(crate::tar::TarPacker::new_gz)?;

                #[cfg(not(feature = "tar-gz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-gz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar.xz" | ".txz") => {
                #[cfg(feature = "tar-xz")]
                self.pack(crate::tar::TarPacker::new_xz)?;

                #[cfg(not(feature = "tar-xz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-xz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".zst" | ".zstd") => {
                #[cfg(feature = "tar-zstd")]
                self.pack(crate::tar::TarPacker::new_zstd)?;

                #[cfg(not(feature = "tar-zstd"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-zstd".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".zip") => {
                #[cfg(feature = "zip")]
                self.pack(crate::zip::ZipPacker::new)?;

                #[cfg(not(feature = "zip"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "zip".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(ext) => {
                return Err(ArchiveError::UnsupportedFormat {
                    format: ext.into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            None => {
                return Err(ArchiveError::UnknownFormat {
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
        };

        Ok(())
    }

    /// Unpack the archive to the destination root, using the provided
    /// unpacker factory. The factory is passed an absolute path
    /// to the output directory, and the input archive file. The unpacked
    /// directory or file is returned from this method.
    ///
    /// When unpacking, we compare files at the destination to those
    /// in the archive, and only unpack the files if they differ.
    /// Furthermore, files at the destination that are not in the
    /// archive are removed entirely.
    pub fn unpack<F, P>(&self, unpacker: F) -> ArchiveResult<()>
    where
        F: FnOnce(&Path, &Path) -> ArchiveResult<P>,
        P: ArchiveUnpacker,
    {
        trace!(
            output_dir = ?self.source_root,
            input_file = ?self.archive_file,
            "Unpacking archive",
        );

        let mut lookup_paths = vec![];
        lookup_paths.extend(self.source_files.values());
        lookup_paths.extend(&self.source_globs);

        let mut differ = TreeDiffer::load(self.source_root, lookup_paths)?;
        let mut archive = unpacker(self.source_root, self.archive_file)?;

        archive.unpack(self.prefix, &mut differ)?;
        differ.remove_stale_tracked_files();

        Ok(())
    }

    /// Determine the unpacker to use based on the archive file extension,
    /// then unpack the archive using [`Archiver.unpack`].
    pub fn unpack_from_ext(&self) -> ArchiveResult<()> {
        match get_full_file_extension(self.archive_file).as_deref() {
            Some(".gz") => {
                #[cfg(feature = "gz")]
                self.unpack(crate::gz::GzUnpacker::new)?;

                #[cfg(not(feature = "gz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "gz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar") => {
                #[cfg(feature = "tar")]
                self.unpack(crate::tar::TarUnpacker::new)?;

                #[cfg(not(feature = "tar"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar.gz" | ".tgz") => {
                #[cfg(feature = "tar-gz")]
                self.unpack(crate::tar::TarUnpacker::new_gz)?;

                #[cfg(not(feature = "tar-gz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-gz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".tar.xz" | ".txz") => {
                #[cfg(feature = "tar-xz")]
                self.unpack(crate::tar::TarUnpacker::new_xz)?;

                #[cfg(not(feature = "tar-xz"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-xz".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".zst" | ".zstd") => {
                #[cfg(feature = "tar-zstd")]
                self.unpack(crate::tar::TarUnpacker::new_zstd)?;

                #[cfg(not(feature = "tar-zstd"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "tar-zstd".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(".zip") => {
                #[cfg(feature = "zip")]
                self.unpack(crate::zip::ZipUnpacker::new)?;

                #[cfg(not(feature = "zip"))]
                return Err(ArchiveError::FeatureNotEnabled {
                    feature: "zip".into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            Some(ext) => {
                return Err(ArchiveError::UnsupportedFormat {
                    format: ext.into(),
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
            None => {
                return Err(ArchiveError::UnknownFormat {
                    path: self.archive_file.to_path_buf(),
                }
                .into());
            }
        };

        Ok(())
    }
}
