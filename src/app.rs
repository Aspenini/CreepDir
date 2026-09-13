//! High-level orchestration: validate input, run the scan, write output.

use std::fs;
use std::io;
use std::path::Path;

use crate::cli;
use crate::config::{OutputFormat, ScanFilter, ScanOptions};
use crate::error::Error;
use crate::output;
use crate::scan::{self, ScanStats};

/// Run an interactive scan, picking folder and output location via file dialogs.
pub fn run_with_dialogs(options: &ScanOptions, filter: &ScanFilter) -> Result<(), Error> {
    let Some(folder) = rfd::FileDialog::new()
        .set_title("Select Folder to Scan")
        .pick_folder()
    else {
        eprintln!("No folder selected.");
        return Ok(());
    };

    let default_name = cli::default_filename(&folder, options.format.extension());
    let Some(output_path) = rfd::FileDialog::new()
        .set_title("Select Output Location")
        .set_file_name(&default_name)
        .save_file()
    else {
        eprintln!("No output location selected.");
        return Ok(());
    };

    run(&folder, &output_path, options, filter)
}

/// Validate the target, scan it, and write the catalog in the chosen format.
pub fn run(
    folder: &Path,
    output_path: &Path,
    options: &ScanOptions,
    filter: &ScanFilter,
) -> Result<(), Error> {
    let meta = fs::metadata(folder).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            Error::not_found(folder)
        } else {
            Error::io("reading folder", e)
        }
    })?;
    if !meta.is_dir() {
        return Err(Error::NotADirectory(folder.to_path_buf()));
    }

    ensure_parent_dir(output_path)?;

    // CSV streams straight to disk (flat memory); text/JSON group in memory first.
    let stats = match options.format {
        OutputFormat::Csv => output::csv::write_streaming(folder, output_path, options, filter)
            .map_err(|e| Error::io("writing output file", e))?,
        OutputFormat::Text | OutputFormat::Json => {
            let (catalog, stats) = scan::catalog(folder, options, filter);
            write_grouped(&catalog, &stats, folder, output_path, options)
                .map_err(|e| Error::io("writing output file", e))?;
            stats
        }
    };

    print_summary(&stats, options);
    println!("Saved to: {}", output_path.display());
    Ok(())
}

/// Write a grouped catalog as text or JSON.
fn write_grouped(
    catalog: &scan::Catalog,
    stats: &ScanStats,
    folder: &Path,
    output_path: &Path,
    options: &ScanOptions,
) -> io::Result<()> {
    match options.format {
        OutputFormat::Json => output::json::write(catalog, stats, folder, output_path, options),
        OutputFormat::Text => output::text::write(catalog, output_path, options),
        OutputFormat::Csv => unreachable!("CSV is streamed, not grouped"),
    }
}

/// Create the output file's parent directory if needed.
fn ensure_parent_dir(output_path: &Path) -> Result<(), Error> {
    let Some(parent) = output_path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(parent).map_err(|e| Error::io("creating output directory", e))
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
