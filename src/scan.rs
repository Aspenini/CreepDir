//! Parallel filesystem traversal that groups files by extension.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use globset::GlobSet;
use jwalk::{Parallelism, WalkDir};

use crate::config::{ScanFilter, ScanOptions};

/// Files grouped by their extension key (e.g. `.rs`).
///
/// Each bucket is sorted by path when produced by [`catalog`].
pub type Catalog = HashMap<String, Vec<FileEntry>>;

/// Summary counts produced by a scan.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    pub symlinks: u64,
    pub total_size: u64,
}

/// A single catalogued file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: Option<u64>,
}

/// Walk `root` recursively, invoke `on_file` for each included file, sort each
/// extension bucket by path, and return the catalog plus [`ScanStats`].
#[must_use]
pub fn catalog(root: &Path, options: &ScanOptions, filter: &ScanFilter) -> (Catalog, ScanStats) {
    let mut catalog = Catalog::new();
    let stats = walk(root, options, filter, |ext, path, size| {
        catalog
            .entry(ext)
            .or_default()
            .push(FileEntry { path, size });
    });
    for entries in catalog.values_mut() {
        entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    }
    (catalog, stats)
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
#[must_use]
pub fn extension_key(path: &Path) -> String {
    let Some(ext) = path.extension().and_then(OsStr::to_str) else {
        return String::new();
    };

    let mut key = String::with_capacity(ext.len() + 1);
    key.push('.');
    if ext.is_ascii() {
        key.extend(ext.bytes().map(|b| b.to_ascii_lowercase() as char));
    } else {
        key.extend(ext.chars().flat_map(char::to_lowercase));
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OutputFormat;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn options(sizes: bool) -> ScanOptions {
        ScanOptions {
            quiet: true,
            threads: Some(1),
            follow_symlinks: false,
            max_depth: None,
            sizes,
            format: OutputFormat::Text,
        }
    }

    fn with_temp_tree(files: &[(&str, &[u8])]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "creepdir-{}-{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (rel, bytes) in files {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, bytes).unwrap();
        }
        dir
    }

    #[test]
    fn extension_key_lowercases_ascii() {
        assert_eq!(extension_key(Path::new("Foo.RS")), ".rs");
        assert_eq!(extension_key(Path::new("a.TXT")), ".txt");
        assert_eq!(extension_key(Path::new("Makefile")), "");
        assert_eq!(extension_key(Path::new("archive.tar.gz")), ".gz");
    }

    #[test]
    fn catalog_groups_by_extension_and_sorts_paths() {
        let root = with_temp_tree(&[
            ("z.rs", b"1"),
            ("a.rs", b"22"),
            ("notes.TXT", b"abc"),
            ("README", b"nope"),
            ("sub/b.rs", b""),
        ]);
        let filter = ScanFilter::new(&[], &[]).unwrap();
        let (catalog, stats) = catalog(&root, &options(true), &filter);

        assert_eq!(stats.files, 5);
        assert!(stats.dirs >= 1);
        assert_eq!(stats.total_size, 1 + 2 + 3 + 4);

        let rs = &catalog[".rs"];
        assert_eq!(
            rs.iter()
                .map(|e| e.path.to_string_lossy().replace('\\', "/"))
                .collect::<Vec<_>>(),
            ["a.rs", "sub/b.rs", "z.rs"]
        );
        assert_eq!(catalog[".txt"].len(), 1);
        assert_eq!(catalog[""].len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ext_and_exclude_filters() {
        let root = with_temp_tree(&[
            ("keep.rs", b"a"),
            ("skip.tmp", b"b"),
            ("node_modules/lib.rs", b"c"),
            ("src/main.rs", b"d"),
        ]);
        let filter =
            ScanFilter::new(&["rs".into()], &["*.tmp".into(), "node_modules".into()]).unwrap();
        let (catalog, stats) = catalog(&root, &options(false), &filter);

        assert_eq!(stats.files, 2);
        assert!(catalog.contains_key(".rs"));
        assert!(!catalog.contains_key(".tmp"));
        let paths: Vec<_> = catalog[".rs"]
            .iter()
            .map(|e| e.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.contains(&"keep.rs".into()));
        assert!(paths.contains(&"src/main.rs".into()));
        assert!(!paths.iter().any(|p| p.contains("node_modules")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn max_depth_zero_is_root_only() {
        let root = with_temp_tree(&[("root.rs", b"a"), ("nested/deep.rs", b"b")]);
        let mut opts = options(false);
        opts.max_depth = Some(0);
        let filter = ScanFilter::new(&[], &[]).unwrap();
        let (catalog, stats) = catalog(&root, &opts, &filter);

        // jwalk's max_depth 0 is the starting dir itself (no children).
        assert_eq!(stats.files, 0, "root-only depth should not list children");
        assert!(catalog.is_empty());

        opts.max_depth = Some(1);
        let (catalog, stats) = super::catalog(&root, &opts, &filter);
        assert_eq!(stats.files, 1);
        assert!(catalog[".rs"].iter().any(|e| e.path.ends_with("root.rs")));

        let _ = fs::remove_dir_all(root);
    }
}
