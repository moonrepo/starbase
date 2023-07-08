use crate::error::ArchiveError;
use crate::join_file_name;
use crate::tree_differ::TreeDiffer;
use rustc_hash::FxHashMap;
use starbase_utils::{fs, glob};
use std::path::{Path, PathBuf};
use tracing::trace;

pub trait ArchivePacker {
    fn add_file(&mut self, name: &str, file: &Path) -> Result<(), ArchiveError>;
    fn add_dir(&mut self, name: &str, dir: &Path) -> Result<(), ArchiveError>;
    fn pack(&mut self) -> Result<(), ArchiveError>;
}

pub trait ArchiveUnpacker {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> Result<(), ArchiveError>;
}

pub struct Archiver<'owner> {
    archive_file: &'owner Path,

    prefix: &'owner str,

    // Relative file path in archive -> absolute file path to source
    source_files: FxHashMap<String, PathBuf>,

    // Relative file prefix in archive -> glob to finds files with
    source_globs: FxHashMap<String, String>,

    source_root: &'owner Path,
}

impl<'owner> Archiver<'owner> {
    pub fn new(source_root: &'owner Path, archive_file: &'owner Path) -> Self {
        Archiver {
            archive_file,
            prefix: "",
            source_files: FxHashMap::default(),
            source_globs: FxHashMap::default(),
            source_root,
        }
    }

    pub fn add_source_file<F: AsRef<Path>>(
        &mut self,
        source: F,
        custom_name: Option<&str>,
    ) -> &mut Self {
        let source = source.as_ref();
        let name = custom_name
            .map(|n| n.to_owned())
            .unwrap_or_else(|| fs::file_name(source));

        self.source_files.insert(name, source.to_path_buf());
        self
    }

    pub fn add_source_glob<G: AsRef<str>>(
        &mut self,
        glob: G,
        custom_prefix: Option<&str>,
    ) -> &mut Self {
        let prefix = custom_prefix.unwrap_or_default().to_owned();

        self.source_globs.insert(prefix, glob.as_ref().to_owned());
        self
    }

    pub fn set_prefix(&mut self, prefix: &'owner str) -> &mut Self {
        self.prefix = prefix;
        self
    }

    pub fn pack<F, P>(&self, packer: F) -> Result<(), ArchiveError>
    where
        F: FnOnce(&Path, &Path) -> P,
        P: ArchivePacker,
    {
        trace!(
            input_dir = ?self.source_root,
            output_file = ?self.archive_file,
            "Packing archive",
        );

        let mut archive = packer(&self.source_root, &self.archive_file);

        for (file, source) in &self.source_files {
            if !source.exists() {
                trace!(source = ?source, "Source file does not exist, skipping");

                continue;
            }

            let name = join_file_name(&[self.prefix, file]);

            if source.is_file() {
                trace!(source = ?source, "Packing file");

                archive.add_file(&name, source)?;
            } else {
                trace!(source = ?source, "Packing directory");

                archive.add_dir(&name, source)?;
            }
        }

        for (file_prefix, glob) in &self.source_globs {
            trace!(glob, "Packing files using glob");

            for file in glob::walk_files(self.source_root, &[glob]).unwrap() {
                let file_name = fs::file_name(file.strip_prefix(self.source_root).unwrap());

                archive.add_file(
                    &join_file_name(&[self.prefix, file_prefix, &file_name]),
                    &file,
                )?;
            }
        }

        archive.pack()?;

        Ok(())
    }

    pub fn unpack<F, P>(&self, unpacker: F) -> Result<(), ArchiveError>
    where
        F: FnOnce(&Path, &Path) -> P,
        P: ArchiveUnpacker,
    {
        trace!(
            output_dir = ?self.source_root,
            input_file = ?self.archive_file,
            "Unpacking archive",
        );

        let mut archive = unpacker(&self.source_root, &self.archive_file);
        let mut differ = TreeDiffer::load(&self.source_root, &["*.test"])?;

        archive.unpack(self.prefix, &mut differ)?;
        differ.remove_stale_tracked_files();

        Ok(())
    }
}
