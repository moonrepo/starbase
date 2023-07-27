use crate::join_file_name;
use crate::tree_differ::TreeDiffer;
use rustc_hash::FxHashMap;
use starbase_utils::{fs, glob};
use std::path::{Path, PathBuf};
use tracing::trace;

pub trait ArchivePacker {
    fn add_file(&mut self, name: &str, file: &Path) -> miette::Result<()>;
    fn add_dir(&mut self, name: &str, dir: &Path) -> miette::Result<()>;
    fn pack(&mut self) -> miette::Result<()>;
}

pub trait ArchiveUnpacker {
    fn unpack(&mut self, prefix: &str, differ: &mut TreeDiffer) -> miette::Result<()>;
}

#[derive(Debug)]
pub struct Archiver<'owner> {
    archive_file: &'owner Path,

    prefix: &'owner str,

    // Absolute file path to source -> Relative file path in archive
    source_files: FxHashMap<PathBuf, String>,

    // Glob to finds files with -> Relative file prefix in archive
    source_globs: FxHashMap<String, String>,

    pub source_root: &'owner Path,
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
        let source = source.strip_prefix(self.source_root).unwrap_or(source);

        self.source_files.insert(
            self.source_root.join(source),
            custom_name
                .map(|n| n.to_owned())
                .unwrap_or_else(|| source.to_string_lossy().to_string()),
        );

        self
    }

    pub fn add_source_glob<G: AsRef<str>>(
        &mut self,
        glob: G,
        custom_prefix: Option<&str>,
    ) -> &mut Self {
        self.source_globs.insert(
            glob.as_ref().to_owned(),
            custom_prefix.unwrap_or_default().to_owned(),
        );
        self
    }

    pub fn set_prefix(&mut self, prefix: &'owner str) -> &mut Self {
        self.prefix = prefix;
        self
    }

    pub fn pack<F, P>(&self, packer: F) -> miette::Result<()>
    where
        F: FnOnce(&Path) -> miette::Result<P>,
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
                trace!(source = ?source, "Packing file");

                archive.add_file(&name, source)?;
            } else {
                trace!(source = ?source, "Packing directory");

                archive.add_dir(&name, source)?;
            }
        }

        for (glob, file_prefix) in &self.source_globs {
            trace!(glob, prefix = file_prefix, "Packing files using glob");

            for file in glob::walk_files(self.source_root, &[glob]).unwrap() {
                let file_name = file
                    .strip_prefix(self.source_root)
                    .unwrap()
                    .to_str()
                    .unwrap();

                archive.add_file(
                    &join_file_name([self.prefix, file_prefix, file_name]),
                    &file,
                )?;
            }
        }

        archive.pack()?;

        Ok(())
    }

    pub fn unpack<F, P>(&self, unpacker: F) -> miette::Result<()>
    where
        F: FnOnce(&Path, &Path) -> miette::Result<P>,
        P: ArchiveUnpacker,
    {
        trace!(
            output_dir = ?self.source_root,
            input_file = ?self.archive_file,
            "Unpacking archive",
        );

        let mut lookup_paths = vec![];
        lookup_paths.extend(self.source_files.values());
        lookup_paths.extend(self.source_globs.keys());

        let mut differ = TreeDiffer::load(self.source_root, lookup_paths)?;
        let mut archive = unpacker(self.source_root, self.archive_file)?;
        let result = archive.unpack(self.prefix, &mut differ);

        if result.is_err() {
            trace!(
                output_dir = ?self.source_root,
                "Failed to unpack archive, removing partially extracted files",
            );

            fs::remove_dir_all(self.source_root)?;
        }

        differ.remove_stale_tracked_files();

        result
    }
}
