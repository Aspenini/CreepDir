//! High-level orchestration: validate input, run the scan, write output.

use crate::config::{OutputFormat, ScanFilter, ScanOptions};
use crate::output;
use crate::scan::{self, Catalog, FileEntry, ScanStats};
use crate::{cli, exit_error};
use std::fs;
use std::path::Path;

/// Run an interactive scan, picking folder and output location via file dialogs.
pub fn run_with_dialogs(options: &ScanOptions, filter: &ScanFilter) {
    let folder = match rfd::FileDialog::new()
        .set_title("Select Folder to Scan")
        .pick_folder()
    {
        Some(path) => path,
        None => {
            eprintln!("No folder selected.");
            std::process::exit(0);
        }
    };

    let default_name = cli::default_filename(&folder, options.format.extension());
    let output_path = match rfd::FileDialog::new()
        .set_title("Select Output Location")
        .set_file_name(&default_name)
        .save_file()
    {
        Some(path) => path,
        None => {
            eprintln!("No output location selected.");
            std::process::exit(0);
        }
    };

    run(&folder, &output_path, options, filter);
}

/// Validate the target, scan it, and write the catalog in the chosen format.
pub fn run(folder: &Path, output_path: &Path, options: &ScanOptions, filter: &ScanFilter) {
    if !folder.exists() {
        eprintln!("Error: Folder '{}' does not exist", folder.display());
        if folder.is_relative()
            && let Ok(cwd) = std::env::current_dir()
        {
            eprintln!("Current directory: {}", cwd.display());
            eprintln!("Tried to resolve: {}", cwd.join(folder).display());
        }
        std::process::exit(1);
    }
    if !folder.is_dir() {
        exit_error(&format!("'{}' is not a directory", folder.display()));
    }

    ensure_parent_dir(output_path);

    // CSV streams straight to disk (flat memory); text/JSON group in memory first.
    let stats = match options.format {
        OutputFormat::Csv => output::csv::write_streaming(folder, output_path, options, filter)
            .unwrap_or_else(|e| exit_error(&format!("writing output file: {e}"))),
        OutputFormat::Text | OutputFormat::Json => {
            let mut catalog: Catalog = Catalog::new();
            let stats = scan::walk(folder, options, filter, |ext, path, size| {
                catalog.entry(ext).or_default().push(FileEntry { path, size });
            });
            write_grouped(&catalog, &stats, folder, output_path, options)
                .unwrap_or_else(|e| exit_error(&format!("writing output file: {e}")));
            stats
        }
    };

    print_summary(&stats, options);
    println!("Saved to: {}", output_path.display());
}

/// Write a grouped catalog as text or JSON.
fn write_grouped(
    catalog: &Catalog,
    stats: &ScanStats,
    folder: &Path,
    output_path: &Path,
    options: &ScanOptions,
) -> std::io::Result<()> {
    match options.format {
        OutputFormat::Json => output::json::write(catalog, stats, folder, output_path, options),
        _ => output::text::write(catalog, output_path, options),
    }
}

/// Create the output file's parent directory if needed.
fn ensure_parent_dir(output_path: &Path) {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = fs::create_dir_all(parent)
    {
        exit_error(&format!("creating output directory: {e}"));
    }
}

/// Print the post-scan summary line to stdout.
fn print_summary(stats: &ScanStats, options: &ScanOptions) {
    if options.sizes {
        println!(
            "Scanned {} files ({}) in {} directories ({} skipped, {} symlinks).",
            stats.files,
            output::human_size(stats.total_size),
            stats.dirs,
            stats.skipped,
            stats.symlinks
        );
    } else {
        println!(
            "Scanned {} files in {} directories ({} skipped, {} symlinks).",
            stats.files, stats.dirs, stats.skipped, stats.symlinks
        );
    }
}
