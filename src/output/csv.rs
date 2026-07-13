//! CSV output, streamed as files are discovered so memory stays flat.

use super::buffered_writer;
use crate::config::{ScanFilter, ScanOptions};
use crate::scan::{self, ScanStats};
use std::io::{self, Write};
use std::path::Path;

/// Scan `root` and stream rows straight to a CSV file, returning the scan stats.
pub fn write_streaming(
    root: &Path,
    output_path: &Path,
    options: &ScanOptions,
    filter: &ScanFilter,
) -> io::Result<ScanStats> {
    let mut writer = buffered_writer(output_path)?;
    writeln!(writer, "extension,path,size")?;

    let mut write_err: Option<io::Error> = None;
    let stats = scan::walk(root, options, filter, |ext, path, size| {
        if write_err.is_some() {
            return;
        }
        let size_field = size.map(|s| s.to_string()).unwrap_or_default();
        if let Err(e) = writeln!(
            writer,
            "{},{},{size_field}",
            field(&ext),
            field(&path.to_string_lossy()),
        ) {
            write_err = Some(e);
        }
    });

    if let Some(e) = write_err {
        return Err(e);
    }
    writer.flush()?;
    Ok(stats)
}

/// Quote a CSV field if it contains a comma, quote, or newline (RFC 4180).
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
