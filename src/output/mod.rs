//! Catalog writers for the supported output formats.

pub mod csv;
pub mod json;
pub mod text;

use crate::scan::Catalog;
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

/// Buffer size for output writers; large enough to keep syscalls rare on big scans.
const WRITER_CAPACITY: usize = 128 * 1024;

/// Create a buffered writer over a freshly created output file.
fn buffered_writer(path: &Path) -> io::Result<BufWriter<File>> {
    Ok(BufWriter::with_capacity(WRITER_CAPACITY, File::create(path)?))
}

/// Extension keys of a catalog, sorted for deterministic output.
fn sorted_extensions(catalog: &Catalog) -> Vec<&String> {
    let mut extensions: Vec<&String> = catalog.keys().collect();
    extensions.sort();
    extensions
}

/// Human-readable byte size (e.g. `1.5 MB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}
