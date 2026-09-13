//! CSV output, streamed as files are discovered so memory stays flat.

use std::io::{self, Write};
use std::path::Path;

use super::buffered_writer;
use crate::config::{ScanFilter, ScanOptions};
use crate::scan::{self, ScanStats};

/// Scan `root` and stream rows straight to a CSV file, returning the scan stats.
pub fn write_streaming(
    root: &Path,
    output_path: &Path,
    options: &ScanOptions,
    filter: &ScanFilter,
) -> io::Result<ScanStats> {
    let mut writer = buffered_writer(output_path)?;
    writer.write_all(b"extension,path,size\n")?;

    let mut write_err = None;
    let stats = scan::walk(root, options, filter, |ext, path, size| {
        if write_err.is_some() {
            return;
        }
        if let Err(e) = write_row(&mut writer, &ext, &path, size) {
            write_err = Some(e);
        }
    });

    if let Some(e) = write_err {
        return Err(e);
    }
    writer.flush()?;
    Ok(stats)
}

fn write_row(w: &mut impl Write, ext: &str, path: &Path, size: Option<u64>) -> io::Result<()> {
    write_field(w, ext)?;
    w.write_all(b",")?;
    write_field(w, &path.to_string_lossy())?;
    w.write_all(b",")?;
    if let Some(size) = size {
        write!(w, "{size}")?;
    }
    w.write_all(b"\n")
}

/// Quote a CSV field if it contains a comma, quote, or newline (RFC 4180).
fn write_field(w: &mut impl Write, s: &str) -> io::Result<()> {
    if s.as_bytes()
        .iter()
        .any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r'))
    {
        w.write_all(b"\"")?;
        let mut rest = s;
        while let Some(i) = rest.find('"') {
            w.write_all(&rest.as_bytes()[..i])?;
            w.write_all(br#""""#)?;
            rest = &rest[i + 1..];
        }
        w.write_all(rest.as_bytes())?;
        w.write_all(b"\"")
    } else {
        w.write_all(s.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(s: &str) -> String {
        let mut buf = Vec::new();
        write_field(&mut buf, s).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn field_quotes_specials() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("a\nb"), "\"a\nb\"");
    }
}
