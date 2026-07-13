//! Command-line interface definition and path resolution helpers.

use crate::config::{OutputFormat, ScanFilter, ScanOptions};
use clap::Parser;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "CreepDir", version)]
#[command(
    about = "Scan folders and catalog files by extension",
    long_about = "CreepDir recursively scans a folder and catalogs files grouped by extension.

Output formats: text (default), --json, or --csv.
Scanning runs in parallel and skips folders it can't access instead of aborting.

Usage modes:
  - Run without arguments to show this help message
  - Use --select to open file dialogs for interactive folder/output selection
  - Provide a folder path to scan it (output saved in the scanned folder)
  - Provide both folder and output paths to specify exact output location"
)]
pub struct Cli {
    /// Path to the folder to scan
    #[arg(value_name = "FOLDER")]
    pub folder: Option<PathBuf>,

    /// Path where the output file should be saved
    #[arg(value_name = "OUTPUT")]
    pub output: Option<PathBuf>,

    /// Open file explorer to select folder and output location
    #[arg(long, short = 's')]
    pub select: bool,

    /// Suppress per-folder "skipping" warnings for inaccessible paths
    #[arg(long, short = 'q')]
    pub quiet: bool,

    /// Number of threads to use for scanning (default: number of CPU cores)
    #[arg(long, short = 'j', value_name = "N")]
    pub threads: Option<usize>,

    /// Follow symlinks and junctions (loops are reported and skipped)
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Maximum directory depth to descend (0 = only the root folder)
    #[arg(long, value_name = "N")]
    pub max_depth: Option<usize>,

    /// Only include these extensions, comma-separated (e.g. --ext rs,txt,md)
    #[arg(long, value_name = "EXTS", value_delimiter = ',')]
    pub ext: Vec<String>,

    /// Exclude paths matching a glob (repeatable, e.g. --exclude "*.tmp" --exclude node_modules)
    #[arg(long, short = 'e', value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Include file sizes in the output and a size total in the summary
    #[arg(long)]
    pub sizes: bool,

    /// Write output as JSON
    #[arg(long, conflicts_with = "csv")]
    pub json: bool,

    /// Write output as CSV (streamed, low memory)
    #[arg(long)]
    pub csv: bool,
}

impl Cli {
    /// Resolve the scan options selected on the command line.
    pub fn scan_options(&self) -> ScanOptions {
        ScanOptions {
            quiet: self.quiet,
            threads: self.threads,
            follow_symlinks: self.follow_symlinks,
            max_depth: self.max_depth,
            sizes: self.sizes,
            format: OutputFormat::from_flags(self.json, self.csv),
        }
    }

    /// Build the include/exclude filter, failing on an invalid glob.
    pub fn scan_filter(&self) -> Result<ScanFilter, String> {
        ScanFilter::new(&self.ext, &self.exclude)
    }
}

/// Resolve the final output file path given the scanned folder and chosen format.
///
/// If `output` is an existing directory, a `<folder>.<ext>` file is placed inside it.
/// If `output` is `None`, the file is written into the scanned folder.
pub fn resolve_output_path(
    output: Option<PathBuf>,
    folder: &Path,
    format: OutputFormat,
) -> PathBuf {
    let ext = format.extension();
    match output {
        Some(output) => {
            let normalized = normalize_path(output);
            if normalized.is_dir() {
                normalized.join(default_filename(folder, ext))
            } else {
                normalized
            }
        }
        None => folder.join(default_filename(folder, ext)),
    }
}

/// Build the default output filename (`<folder>.<ext>`) for a scanned folder.
pub fn default_filename(folder: &Path, ext: &str) -> String {
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");
    format!("{name}.{ext}")
}

/// Normalize and resolve a path to work cross-platform.
///
/// On Windows, paths that start with `/` but aren't valid absolute paths are treated
/// as relative to the current directory. Relative paths are resolved against the CWD.
pub fn normalize_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy();
        // Windows absolute paths are `C:\...` or `\\server\share`. A leading `/`
        // that isn't a drive/UNC marker is treated as relative to the CWD.
        if path_str.starts_with('/') && path_str.len() > 1 {
            let second = path_str.as_bytes()[1];
            if second != b':' && second != b'/' && second != b'\\' {
                let relative = PathBuf::from(&path_str[1..]);
                if let Ok(cwd) = env::current_dir() {
                    return cwd.join(relative);
                }
            }
        }
    }

    if path.is_absolute() {
        return path;
    }
    if let Ok(cwd) = env::current_dir() {
        return cwd.join(&path);
    }
    path
}
