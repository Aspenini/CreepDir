//! JSON output: a summary plus files grouped by extension.

use super::{buffered_writer, sorted_extensions};
use crate::config::ScanOptions;
use crate::scan::{Catalog, ScanStats};
use std::io::{self, Write};
use std::path::Path;

pub fn write(
    catalog: &Catalog,
    stats: &ScanStats,
    root: &Path,
    output_path: &Path,
    options: &ScanOptions,
) -> io::Result<()> {
    let mut w = buffered_writer(output_path)?;

    writeln!(w, "{{")?;
    writeln!(w, "  \"generator\": \"CreepDir {}\",", env!("CARGO_PKG_VERSION"))?;
    writeln!(w, "  \"root\": \"{}\",", escape(&root.to_string_lossy()))?;
    writeln!(w, "  \"summary\": {{")?;
    writeln!(w, "    \"files\": {},", stats.files)?;
    writeln!(w, "    \"directories\": {},", stats.dirs)?;
    writeln!(w, "    \"skipped\": {},", stats.skipped)?;
    writeln!(w, "    \"symlinks\": {},", stats.symlinks)?;
    writeln!(w, "    \"total_size\": {}", num_opt(options.sizes, stats.total_size))?;
    writeln!(w, "  }},")?;
    writeln!(w, "  \"extensions\": [")?;

    let extensions = sorted_extensions(catalog);
    for (i, ext) in extensions.iter().enumerate() {
        let mut entries: Vec<_> = catalog[*ext].iter().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let total: u64 = entries.iter().filter_map(|e| e.size).sum();

        writeln!(w, "    {{")?;
        writeln!(w, "      \"extension\": \"{}\",", escape(ext))?;
        writeln!(w, "      \"count\": {},", entries.len())?;
        writeln!(w, "      \"total_size\": {},", num_opt(options.sizes, total))?;
        writeln!(w, "      \"files\": [")?;
        for (j, entry) in entries.iter().enumerate() {
            let comma = if j + 1 < entries.len() { "," } else { "" };
            let size = entry.size.map_or_else(|| "null".to_string(), |s| s.to_string());
            writeln!(
                w,
                "        {{ \"path\": \"{}\", \"size\": {size} }}{comma}",
                escape(&entry.path.to_string_lossy()),
            )?;
        }
        writeln!(w, "      ]")?;
        let comma = if i + 1 < extensions.len() { "," } else { "" };
        writeln!(w, "    }}{comma}")?;
    }

    writeln!(w, "  ]")?;
    writeln!(w, "}}")?;
    w.flush()
}

/// A numeric JSON value when `present`, otherwise `null`.
fn num_opt(present: bool, value: u64) -> String {
    if present {
        value.to_string()
    } else {
        "null".to_string()
    }
}

/// Escape a string for inclusion in a JSON string literal.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
