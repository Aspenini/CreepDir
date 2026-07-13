//! Plain-text output: files grouped under `--- .ext (N files) ---` headers.

use super::{buffered_writer, human_size, sorted_extensions};
use crate::config::ScanOptions;
use crate::scan::Catalog;
use std::io::{self, Write};
use std::path::Path;

pub fn write(catalog: &Catalog, output_path: &Path, options: &ScanOptions) -> io::Result<()> {
    let mut writer = buffered_writer(output_path)?;

    for ext in sorted_extensions(catalog) {
        let mut entries: Vec<_> = catalog[ext].iter().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let label = if ext.is_empty() { "(no extension)" } else { ext };
        if options.sizes {
            let total: u64 = entries.iter().filter_map(|e| e.size).sum();
            writeln!(
                writer,
                "--- {label} ({} files, {}) ---",
                entries.len(),
                human_size(total)
            )?;
        } else {
            writeln!(writer, "--- {label} ({} files) ---", entries.len())?;
        }

        for entry in entries {
            if options.sizes {
                writeln!(
                    writer,
                    "{}\t{}",
                    entry.path.to_string_lossy(),
                    human_size(entry.size.unwrap_or(0))
                )?;
            } else {
                writeln!(writer, "{}", entry.path.to_string_lossy())?;
            }
        }
        writeln!(writer)?;
    }

    writer.flush()
}
