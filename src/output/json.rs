//! JSON output: a summary plus files grouped by extension.

use std::io::{self, Write};
use std::path::Path;

use super::{buffered_writer, sorted_extensions};
use crate::config::ScanOptions;
use crate::scan::{Catalog, ScanStats};

/// Write `catalog` as JSON. Each extension bucket is assumed sorted by path.
pub fn write(
    catalog: &Catalog,
    stats: &ScanStats,
    root: &Path,
    output_path: &Path,
    options: &ScanOptions,
) -> io::Result<()> {
    let mut w = buffered_writer(output_path)?;

    writeln!(w, "{{")?;
    write!(w, "  \"generator\": ")?;
    write_string(&mut w, concat!("CreepDir ", env!("CARGO_PKG_VERSION")))?;
    writeln!(w, ",")?;
    write!(w, "  \"root\": ")?;
    write_string(&mut w, &root.to_string_lossy())?;
    writeln!(w, ",")?;
    writeln!(w, "  \"summary\": {{")?;
    writeln!(w, "    \"files\": {},", stats.files)?;
    writeln!(w, "    \"directories\": {},", stats.dirs)?;
    writeln!(w, "    \"skipped\": {},", stats.skipped)?;
    writeln!(w, "    \"symlinks\": {},", stats.symlinks)?;
    write!(w, "    \"total_size\": ")?;
    write_opt_u64(&mut w, options.sizes, stats.total_size)?;
    writeln!(w)?;
    writeln!(w, "  }},")?;
    writeln!(w, "  \"extensions\": [")?;

    let extensions = sorted_extensions(catalog);
    for (i, ext) in extensions.iter().enumerate() {
        let entries = &catalog[*ext];
        let total: u64 = entries.iter().filter_map(|e| e.size).sum();

        writeln!(w, "    {{")?;
        write!(w, "      \"extension\": ")?;
        write_string(&mut w, ext)?;
        writeln!(w, ",")?;
        writeln!(w, "      \"count\": {},", entries.len())?;
        write!(w, "      \"total_size\": ")?;
        write_opt_u64(&mut w, options.sizes, total)?;
        writeln!(w, ",")?;
        writeln!(w, "      \"files\": [")?;
        for (j, entry) in entries.iter().enumerate() {
            write!(w, "        {{ \"path\": ")?;
            write_string(&mut w, &entry.path.to_string_lossy())?;
            write!(w, ", \"size\": ")?;
            match entry.size {
                Some(s) => write!(w, "{s}")?,
                None => w.write_all(b"null")?,
            }
            write!(w, " }}")?;
            if j + 1 < entries.len() {
                writeln!(w, ",")?;
            } else {
                writeln!(w)?;
            }
        }
        writeln!(w, "      ]")?;
        write!(w, "    }}")?;
        if i + 1 < extensions.len() {
            writeln!(w, ",")?;
        } else {
            writeln!(w)?;
        }
    }

    writeln!(w, "  ]")?;
    writeln!(w, "}}")?;
    w.flush()
}

fn write_opt_u64(w: &mut impl Write, present: bool, value: u64) -> io::Result<()> {
    if present {
        write!(w, "{value}")
    } else {
        w.write_all(b"null")
    }
}

/// Write `s` as a JSON string literal (including surrounding quotes).
fn write_string(w: &mut impl Write, s: &str) -> io::Result<()> {
    w.write_all(b"\"")?;
    let bytes = s.as_bytes();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        let esc: &[u8] = match c {
            '"' => b"\\\"",
            '\\' => b"\\\\",
            '\n' => b"\\n",
            '\r' => b"\\r",
            '\t' => b"\\t",
            c if (c as u32) < 0x20 => {
                w.write_all(&bytes[start..i])?;
                write!(w, "\\u{:04x}", c as u32)?;
                start = i + c.len_utf8();
                continue;
            }
            _ => continue,
        };
        w.write_all(&bytes[start..i])?;
        w.write_all(esc)?;
        start = i + c.len_utf8();
    }
    w.write_all(&bytes[start..])?;
    w.write_all(b"\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escaped(s: &str) -> String {
        let mut buf = Vec::new();
        write_string(&mut buf, s).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn write_string_quotes_and_escapes() {
        assert_eq!(escaped("plain"), "\"plain\"");
        assert_eq!(escaped("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(escaped("a\nb\tc"), r#""a\nb\tc""#);
        assert_eq!(escaped("\u{0007}"), r#""\u0007""#);
    }
}
