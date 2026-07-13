//! Parallel filesystem traversal that groups files by extension.

use crate::config::{ScanFilter, ScanOptions};
use globset::GlobSet;
use jwalk::{Parallelism, WalkDir};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Files grouped by their extension key (e.g. `.rs`).
pub type Catalog = HashMap<String, Vec<FileEntry>>;

/// Summary counts produced by a scan.
#[derive(Default)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    pub symlinks: u64,
    pub total_size: u64,
}

/// A single catalogued file.
pub struct FileEntry {
    pub path: PathBuf,
    pub size: Option<u64>,
}

/// Walk `root` recursively, invoking `on_file(ext, relative_path, size)` for each
/// included file, and return the resulting [`ScanStats`].
///
/// Directory reads run in parallel on a thread pool via `jwalk`. Inaccessible
/// directories/entries are skipped (counted, and warned about unless `--quiet`)
/// rather than aborting the scan. By default symlinks/junctions are not followed;
/// with `--follow-symlinks` jwalk follows them and reports loops as errors.
pub fn walk<F>(root: &Path, options: &ScanOptions, filter: &ScanFilter, mut on_file: F) -> ScanStats
where
    F: FnMut(String, PathBuf, Option<u64>),
{
    let mut stats = ScanStats::default();

    let mut walker = WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(options.follow_symlinks);

    if let Some(n) = options.threads {
        walker = walker.parallelism(Parallelism::RayonNewPool(n.max(1)));
    }
    if let Some(depth) = options.max_depth {
        walker = walker.max_depth(depth);
    }

    // Prune excluded paths during traversal so we never descend into them.
    if let Some(exclude) = filter.exclude() {
        let exclude = Arc::clone(exclude);
        let root_buf = root.to_path_buf();
        walker = walker.process_read_dir(move |_depth, _path, _state, children| {
            children.retain(|res| match res {
                Ok(child) => !path_excluded(&exclude, &root_buf, &child.path(), &child.file_name),
                Err(_) => true,
            });
        });
    }

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                stats.skipped += 1;
                if !options.quiet {
                    match e.path() {
                        Some(p) => eprintln!("Warning: skipping '{}': {e}", p.display()),
                        None => eprintln!("Warning: skipping entry: {e}"),
                    }
                }
                continue;
            }
        };

        // A symlink we're not following is neither descended nor catalogued.
        if entry.path_is_symlink() && !options.follow_symlinks {
            stats.symlinks += 1;
            continue;
        }

        let file_type = entry.file_type();
        if file_type.is_dir() {
            stats.dirs += 1;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let ext = extension_key(&path);
        if !filter.ext_allowed(&ext) {
            continue;
        }

        let size = if options.sizes {
            entry.metadata().ok().map(|m| m.len())
        } else {
            None
        };
        if let Some(s) = size {
            stats.total_size += s;
        }

        let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        stats.files += 1;
        on_file(ext, relative_path, size);
    }

    stats
}

/// Return true if the entry matches any exclude glob. Matched against both the bare
/// file name and the forward-slash relative path, so `*.tmp` matches by name while
/// `build/**` can target a subtree.
fn path_excluded(exclude: &GlobSet, root: &Path, full: &Path, file_name: &OsStr) -> bool {
    if exclude.is_match(Path::new(file_name)) {
        return true;
    }
    let rel = full.strip_prefix(root).unwrap_or(full);
    let normalized = rel.to_string_lossy().replace('\\', "/");
    exclude.is_match(normalized)
}

/// Build the lowercase, dot-prefixed extension key for a file (e.g. `.txt`).
/// Files without an extension map to an empty string.
pub fn extension_key(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => {
            let mut key = String::with_capacity(ext.len() + 1);
            key.push('.');
            for c in ext.chars() {
                key.extend(c.to_lowercase());
            }
            key
        }
        None => String::new(),
    }
}
