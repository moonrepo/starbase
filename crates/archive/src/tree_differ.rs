use crate::error::ArchiveError;
use rustc_hash::FxHashMap;
use starbase_utils::{fs, glob};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use tracing::trace;

pub struct TreeDiffer {
    /// A mapping of all files in the destination directory
    /// to their current file sizes.
    pub files: FxHashMap<PathBuf, u64>,
}

impl TreeDiffer {
    /// Load the tree at the defined destination root and scan the file system
    /// using the defined lists of paths, either files, folders, or globs. If a folder,
    /// recursively scan all files and create an internal manifest to track diffing.
    pub fn load<P, I, V>(dest_root: P, lookup_paths: I) -> Result<Self, ArchiveError>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = V>,
        V: AsRef<str>,
    {
        let mut files = FxHashMap::default();
        let dest_root = dest_root.as_ref();

        trace!(dir = %dest_root.display(), "Creating a tree differ for destination directory");

        let mut track = |file: PathBuf| {
            if file.exists() {
                let size = match fs::metadata(&file) {
                    Ok(meta) => meta.len(),
                    Err(_) => 0,
                };

                files.insert(file, size);
            }
        };

        let mut globs = vec![];

        for path in lookup_paths {
            let path = path.as_ref();

            if glob::is_glob(path) {
                globs.push(path.to_owned());
            } else {
                let path = dest_root.join(path);

                if path.is_file() {
                    trace!(file = %path.display(), "Tracking file");

                    track(path);
                } else if path.is_dir() {
                    trace!(dir = %path.display(), "Tracking directory");

                    for file in fs::read_dir_all(path)? {
                        track(file.path());
                    }
                }
            }
        }

        if !globs.is_empty() {
            trace!(
                root = %dest_root.display(),
                globs = globs.join(", "),
                "Tracking files using glob",
            );

            for file in glob::walk_files(dest_root, &globs)? {
                track(file);
            }
        }

        Ok(TreeDiffer { files })
    }

    /// Compare 2 files byte-by-byte and return true if both files are equal.
    pub fn are_files_equal<S: Read, D: Read>(&self, source: &mut S, dest: &mut D) -> bool {
        let mut areader = BufReader::new(source);
        let mut breader = BufReader::new(dest);
        let mut abuf = [0; 512];
        let mut bbuf = [0; 512];

        while let (Ok(av), Ok(bv)) = (areader.read(&mut abuf), breader.read(&mut bbuf)) {
            // We've reached the end of the file for either one
            if av < 512 || bv < 512 {
                return abuf == bbuf;
            }

            // Otherwise, compare buffer
            if abuf != bbuf {
                return false;
            }
        }

        false
    }

    /// Remove all files in the destination directory that have not been
    /// overwritten with a source file, or are the same size as a source file.
    /// We can assume these are stale artifacts that should no longer exist!
    pub fn remove_stale_tracked_files(&mut self) {
        trace!("Removing stale and invalid files");

        for (file, _) in self.files.drain() {
            let _ = fs::remove_file(file);
        }
    }

    /// Determine whether the source should be written to the destination.
    /// If a file exists at the destination, run a handful of checks to
    /// determine whether we overwrite the file or keep it (equal content).
    pub fn should_write_source<T: Read>(
        &self,
        source_size: u64,
        source: &mut T,
        dest_path: &Path,
    ) -> Result<bool, ArchiveError> {
        // If the destination doesn't exist, always use the source
        if !dest_path.exists() || !self.files.contains_key(dest_path) {
            return Ok(true);
        }

        // If the file sizes are different, use the source
        let Some(dest_size) = self.files.get(dest_path) else {
            return Ok(true);
        };

        if source_size != *dest_size {
            return Ok(true);
        }

        // If the file sizes are the same, compare byte ranges to determine a difference
        let mut dest = fs::open_file(dest_path)?;

        Ok(!self.are_files_equal(source, &mut dest))
    }

    /// Untrack a destination file from the internal registry.
    pub fn untrack_file(&mut self, dest: &Path) {
        self.files.remove(dest);
    }
}