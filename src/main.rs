use clap::{CommandFactory, Parser};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "CreepDir")]
#[command(
    about = "Scan folders and catalog files by extension",
    long_about = "CreepDir is a CLI tool that scans a folder recursively and catalogs all files by their extension.
The output is saved as a text file with files grouped by extension.

Usage modes:
  - Run without arguments to show this help message
  - Use --select to open file dialogs for interactive folder/output selection
  - Provide a folder path to scan it (output saved in the scanned folder)
  - Provide both folder and output paths to specify exact output location"
)]
struct Cli {
    /// Path to the folder to scan
    #[arg(value_name = "FOLDER")]
    folder: Option<PathBuf>,

    /// Path where the output file should be saved
    #[arg(value_name = "OUTPUT")]
    output: Option<PathBuf>,

    /// Open file explorer to select folder and output location
    #[arg(long, short = 's')]
    select: bool,
}

fn main() {
    let cli = Cli::parse();

    // If --select flag is used, use dialog-based selection
    if cli.select {
        if cli.folder.is_some() || cli.output.is_some() {
            eprintln!("Error: --select cannot be used with path arguments");
            std::process::exit(1);
        }
        run_with_dialogs();
        return;
    }

    // If no arguments provided, show help
    if cli.folder.is_none() {
        Cli::command().print_help().unwrap();
        return;
    }

    let folder = normalize_path(cli.folder.unwrap());

    // Determine output path
    let output_path = if let Some(output) = cli.output {
        let normalized_output = normalize_path(output);
        // If output exists and is a directory, create filename based on folder name
        if normalized_output.is_dir() {
            let folder_name = folder.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("output");
            normalized_output.join(format!("{}.txt", folder_name))
        } else {
            // Output is a specific file path (may or may not exist yet)
            normalized_output
        }
    } else {
        // Default: save in the scanned folder
        let folder_name = folder.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output");
        folder.join(format!("{}.txt", folder_name))
    };

    scan_folder(&folder, &output_path);
}

fn run_with_dialogs() {
    // Select input folder
    let folder_path = rfd::FileDialog::new()
        .set_title("Select Folder to Scan")
        .pick_folder();

    let folder_path = match folder_path {
        Some(path) => path,
        None => {
            eprintln!("No folder selected.");
            std::process::exit(0);
        }
    };

    // Select output location
    let default_filename = format!("{}.txt", 
        folder_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output"));

    let output_path = rfd::FileDialog::new()
        .set_title("Select Output Location")
        .set_file_name(&default_filename)
        .save_file();

    let output_path = match output_path {
        Some(path) => path,
        None => {
            eprintln!("No output location selected.");
            std::process::exit(0);
        }
    };

    scan_folder(&folder_path, &output_path);
}

/// Normalize and resolve a path to work cross-platform
/// Handles paths starting with / on Windows by treating them as relative
fn normalize_path(path: PathBuf) -> PathBuf {
    // On Windows, if path starts with / but isn't a valid absolute path (no drive letter),
    // treat it as relative to current directory
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy().to_string();
        // Check if it's a Unix-style path that starts with / but isn't a valid Windows path
        // Windows absolute paths are: C:\... or \\server\share
        if path_str.starts_with('/') && path_str.len() > 1 {
            let second_char = path_str.chars().nth(1);
            // If second character isn't ':' (drive letter) or '/' (UNC path), it's invalid on Windows
            // Convert to relative by stripping the leading /
            if second_char != Some(':') && second_char != Some('/') && second_char != Some('\\') {
                let relative = PathBuf::from(&path_str[1..]);
                if let Ok(cwd) = env::current_dir() {
                    return cwd.join(&relative);
                }
            }
        }
    }

    // If path is already absolute, return it as-is
    if path.is_absolute() {
        return path;
    }

    // If path is relative, resolve it relative to current directory
    if let Ok(cwd) = env::current_dir() {
        return cwd.join(&path);
    }

    // Fallback: return the original path
    path
}

fn scan_folder(folder: &Path, output_path: &Path) {
    // Validate input folder exists
    if !folder.exists() {
        eprintln!("Error: Folder '{}' does not exist", folder.display());
        if folder.is_relative() {
            if let Ok(cwd) = env::current_dir() {
                eprintln!("Current directory: {}", cwd.display());
                eprintln!("Tried to resolve: {}", cwd.join(folder).display());
            }
        }
        std::process::exit(1);
    }

    if !folder.is_dir() {
        eprintln!("Error: '{}' is not a directory", folder.display());
        std::process::exit(1);
    }

    // Scan folder and group files by extension
    let mut files_by_ext: HashMap<String, Vec<PathBuf>> = HashMap::new();

    if let Err(e) = walk_folder(&folder, &folder, &mut files_by_ext) {
        eprintln!("Error scanning folder: {}", e);
        std::process::exit(1);
    }

    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("Error creating output directory: {}", e);
            std::process::exit(1);
        }
    }

    // Write results to file
    if let Err(e) = write_output(&files_by_ext, output_path) {
        eprintln!("Error writing output file: {}", e);
        std::process::exit(1);
    }

    println!("Saved to: {}", output_path.display());
}

fn walk_folder(
    root: &Path,
    current: &Path,
    files_by_ext: &mut HashMap<String, Vec<PathBuf>>,
) -> Result<(), std::io::Error> {
    let entries = fs::read_dir(current)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_folder(root, &path, files_by_ext)?;
        } else if path.is_file() {
            let extension = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|s| format!(".{}", s.to_lowercase()))
                .unwrap_or_else(|| "".to_string());

            let relative_path = path.strip_prefix(root)
                .unwrap_or(&path)
                .to_path_buf();

            files_by_ext
                .entry(extension)
                .or_insert_with(Vec::new)
                .push(relative_path);
        }
    }

    Ok(())
}

fn write_output(files_by_ext: &HashMap<String, Vec<PathBuf>>, output_path: &Path) -> Result<(), std::io::Error> {
    let mut output = String::new();

    // Sort extensions alphabetically
    let mut extensions: Vec<_> = files_by_ext.keys().collect();
    extensions.sort();

    for ext in extensions {
        let paths = &files_by_ext[ext];
        output.push_str(&format!("--- {} ---\n", ext));
        for path in paths {
            // Convert path to string, handling different path separators
            let path_str = path.to_string_lossy();
            output.push_str(&format!("{}\n", path_str));
        }
        output.push('\n');
    }

    fs::write(output_path, output)?;
    Ok(())
}

