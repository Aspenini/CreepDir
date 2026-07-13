use clap::{CommandFactory, Parser};
use globset::{Glob, GlobSet, GlobSetBuilder};
use jwalk::{Parallelism, WalkDir};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Output format for the catalog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
    Csv,
}

impl OutputFormat {
    /// Default file extension for this format.
    fn extension(self) -> &'static str {
        match self {
            OutputFormat::Text => "txt",
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
        }
    }
}

/// Summary of a folder scan.
#[derive(Default)]
struct ScanStats {
    files: u64,
    dirs: u64,
    skipped: u64,
    symlinks: u64,
    total_size: u64,
}

/// Options that control how a scan is performed.
#[derive(Clone, Copy)]
struct ScanOptions {
    /// Suppress per-folder warnings for inaccessible paths.
    quiet: bool,
    /// Number of worker threads (None = one per CPU core).
    threads: Option<usize>,
    /// Follow symlinks/junctions (jwalk reports loops as errors).
    follow_symlinks: bool,
    /// Maximum recursion depth (None = unlimited). Depth 0 is the root itself.
    max_depth: Option<usize>,
    /// Collect file sizes (costs one metadata call per file).
    sizes: bool,
    /// Output format.
    format: OutputFormat,
}

/// Filters that decide which files are included in the catalog.
struct ScanFilter {
    /// Allow-list of dot-prefixed lowercase extensions (e.g. ".rs"). None = all.
    ext: Option<Vec<String>>,
    /// Glob patterns; matching files/dirs are excluded (dirs are pruned).
    exclude: Option<Arc<GlobSet>>,
}

impl ScanFilter {
    fn ext_allowed(&self, ext: &str) -> bool {
        match &self.ext {
            Some(allowed) => allowed.iter().any(|e| e == ext),
            None => true,
        }
    }
}

/// A single catalogued file.
struct FileEntry {
    path: PathBuf,
    size: Option<u64>,
}

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

    /// Suppress per-folder "skipping" warnings for inaccessible paths
    #[arg(long, short = 'q')]
    quiet: bool,

    /// Number of threads to use for scanning (default: number of CPU cores)
    #[arg(long, short = 'j', value_name = "N")]
    threads: Option<usize>,

    /// Follow symlinks and junctions (loops are reported and skipped)
    #[arg(long)]
    follow_symlinks: bool,

    /// Maximum directory depth to descend (0 = only the root folder)
    #[arg(long, value_name = "N")]
    max_depth: Option<usize>,

    /// Only include these extensions, comma-separated (e.g. --ext rs,txt,md)
    #[arg(long, value_name = "EXTS", value_delimiter = ',')]
    ext: Vec<String>,

    /// Exclude paths matching a glob (repeatable, e.g. --exclude "*.tmp" --exclude node_modules)
    #[arg(long, short = 'e', value_name = "GLOB")]
    exclude: Vec<String>,

    /// Include file sizes in the output and a size total in the summary
    #[arg(long)]
    sizes: bool,

    /// Write output as JSON
    #[arg(long, conflicts_with = "csv")]
    json: bool,

    /// Write output as CSV (streamed, low memory)
    #[arg(long)]
    csv: bool,
}

fn main() {
    let cli = Cli::parse();

    let format = if cli.json {
        OutputFormat::Json
    } else if cli.csv {
        OutputFormat::Csv
    } else {
        OutputFormat::Text
    };

    let options = ScanOptions {
        quiet: cli.quiet,
        threads: cli.threads,
        follow_symlinks: cli.follow_symlinks,
        max_depth: cli.max_depth,
        sizes: cli.sizes,
        format,
    };

    let filter = build_filter(&cli.ext, &cli.exclude);

    // If --select flag is used, use dialog-based selection
    if cli.select {
        if cli.folder.is_some() || cli.output.is_some() {
            eprintln!("Error: --select cannot be used with path arguments");
            std::process::exit(1);
        }
        run_with_dialogs(options, &filter);
        return;
    }

    // If no arguments provided, show help
    if cli.folder.is_none() {
        Cli::command().print_help().unwrap();
        return;
    }

    let folder = normalize_path(cli.folder.unwrap());
    let ext = options.format.extension();

    // Determine output path
    let output_path = if let Some(output) = cli.output {
        let normalized_output = normalize_path(output);
        // If output exists and is a directory, create filename based on folder name
        if normalized_output.is_dir() {
            normalized_output.join(default_filename(&folder, ext))
        } else {
            // Output is a specific file path (may or may not exist yet)
            normalized_output
        }
    } else {
        // Default: save in the scanned folder
        folder.join(default_filename(&folder, ext))
    };

    scan_folder(&folder, &output_path, options, &filter);
}

/// Build the default output filename ("<folder>.<ext>") for a scanned folder.
fn default_filename(folder: &Path, ext: &str) -> String {
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("output");
    format!("{}.{}", name, ext)
}

/// Parse `--ext` and `--exclude` CLI values into a `ScanFilter`.
fn build_filter(ext_args: &[String], exclude_args: &[String]) -> ScanFilter {
    let ext = if ext_args.is_empty() {
        None
    } else {
        Some(
            ext_args
                .iter()
                .map(|e| {
                    let trimmed = e.trim().trim_start_matches('.').to_lowercase();
                    format!(".{}", trimmed)
                })
                .collect(),
        )
    };

    let exclude = if exclude_args.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pattern in exclude_args {
            match Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                }
                Err(e) => {
                    eprintln!("Error: invalid --exclude pattern '{}': {}", pattern, e);
                    std::process::exit(1);
                }
            }
        }
        match builder.build() {
            Ok(set) => Some(Arc::new(set)),
            Err(e) => {
                eprintln!("Error building exclude patterns: {}", e);
                std::process::exit(1);
            }
        }
    };

    ScanFilter { ext, exclude }
}

fn run_with_dialogs(options: ScanOptions, filter: &ScanFilter) {
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
    let default_name = default_filename(&folder_path, options.format.extension());

    let output_path = rfd::FileDialog::new()
        .set_title("Select Output Location")
        .set_file_name(&default_name)
        .save_file();

    let output_path = match output_path {
        Some(path) => path,
        None => {
            eprintln!("No output location selected.");
            std::process::exit(0);
        }
    };

    scan_folder(&folder_path, &output_path, options, filter);
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

fn scan_folder(folder: &Path, output_path: &Path, options: ScanOptions, filter: &ScanFilter) {
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

    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("Error creating output directory: {}", e);
                std::process::exit(1);
            }
        }
    }

    // CSV is streamed straight to disk so memory stays flat even on huge trees.
    // Text and JSON group by extension, which requires buffering the catalog.
    let stats = match options.format {
        OutputFormat::Csv => match scan_to_csv(folder, output_path, options, filter) {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("Error writing output file: {}", e);
                std::process::exit(1);
            }
        },
        OutputFormat::Text | OutputFormat::Json => {
            let mut files_by_ext: HashMap<String, Vec<FileEntry>> = HashMap::new();
            let stats = walk_folder(folder, options, filter, |ext, path, size| {
                files_by_ext
                    .entry(ext)
                    .or_default()
                    .push(FileEntry { path, size });
            });

            let result = if options.format == OutputFormat::Json {
                write_json(&files_by_ext, &stats, folder, output_path, options)
            } else {
                write_text(&files_by_ext, output_path, options)
            };
            if let Err(e) = result {
                eprintln!("Error writing output file: {}", e);
                std::process::exit(1);
            }
            stats
        }
    };

    print_summary(&stats, options);
    println!("Saved to: {}", output_path.display());
}

/// Print the post-scan summary line to stdout.
fn print_summary(stats: &ScanStats, options: ScanOptions) {
    if options.sizes {
        println!(
            "Scanned {} files ({}) in {} directories ({} skipped, {} symlinks).",
            stats.files,
            human_size(stats.total_size),
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

/// Walk `root` recursively, invoking `on_file(ext, relative_path, size)` for each
/// included file.
///
/// Directory reads run in parallel on a thread pool via `jwalk`. Inaccessible
/// directories/entries are skipped (counted, and warned about unless `--quiet`)
/// rather than aborting the scan. By default symlinks/junctions are not followed;
/// with `--follow-symlinks` jwalk follows them and reports loops as errors.
fn walk_folder<F>(root: &Path, options: ScanOptions, filter: &ScanFilter, mut on_file: F) -> ScanStats
where
    F: FnMut(String, PathBuf, Option<u64>),
{
    let mut stats = ScanStats::default();

    let mut walker = WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(options.follow_symlinks);

    if let Some(n) = options.threads {
        walker = walker.parallelism(Parallelism::RayonNewPool(n.max(1)));
    }
    if let Some(depth) = options.max_depth {
        walker = walker.max_depth(depth);
    }

    // Prune excluded paths during traversal so we never descend into them.
    if let Some(exclude) = &filter.exclude {
        let exclude = Arc::clone(exclude);
        let root_buf = root.to_path_buf();
        walker = walker.process_read_dir(move |_depth, _path, _state, children| {
            children.retain(|res| match res {
                Ok(child) => !path_excluded(&exclude, &root_buf, &child.path(), &child.file_name),
                Err(_) => true,
            });
        });
    }

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                stats.skipped += 1;
                if !options.quiet {
                    match e.path() {
                        Some(p) => eprintln!("Warning: skipping '{}': {}", p.display(), e),
                        None => eprintln!("Warning: skipping entry: {}", e),
                    }
                }
                continue;
            }
        };

        // A symlink we're not following is neither descended nor catalogued.
        if entry.path_is_symlink() && !options.follow_symlinks {
            stats.symlinks += 1;
            continue;
        }

        let file_type = entry.file_type();
        if file_type.is_dir() {
            stats.dirs += 1;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let ext = extension_key(&path);
        if !filter.ext_allowed(&ext) {
            continue;
        }

        let size = if options.sizes {
            entry.metadata().ok().map(|m| m.len())
        } else {
            None
        };
        if let Some(s) = size {
            stats.total_size += s;
        }

        let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        stats.files += 1;
        on_file(ext, relative_path, size);
    }

    stats
}

/// Return true if `full`/`file_name` matches any exclude glob (matched against the
/// forward-slash relative path and against the bare file name).
fn path_excluded(
    exclude: &GlobSet,
    root: &Path,
    full: &Path,
    file_name: &std::ffi::OsStr,
) -> bool {
    if exclude.is_match(Path::new(file_name)) {
        return true;
    }
    let rel = full.strip_prefix(root).unwrap_or(full);
    let normalized = rel.to_string_lossy().replace('\\', "/");
    exclude.is_match(normalized)
}

/// Build the lowercase, dot-prefixed extension key for a file (e.g. ".txt").
/// Files without an extension map to an empty string.
fn extension_key(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => {
            let mut key = String::with_capacity(ext.len() + 1);
            key.push('.');
            for c in ext.chars() {
                key.extend(c.to_lowercase());
            }
            key
        }
        None => String::new(),
    }
}

/// Human-readable byte size (e.g. "1.5 MB").
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", size, UNITS[unit])
}

/// Sorted extension keys from the catalog.
fn sorted_extensions(files_by_ext: &HashMap<String, Vec<FileEntry>>) -> Vec<&String> {
    let mut extensions: Vec<&String> = files_by_ext.keys().collect();
    extensions.sort();
    extensions
}

fn write_text(
    files_by_ext: &HashMap<String, Vec<FileEntry>>,
    output_path: &Path,
    options: ScanOptions,
) -> io::Result<()> {
    let file = fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);

    for ext in sorted_extensions(files_by_ext) {
        let mut entries: Vec<&FileEntry> = files_by_ext[ext].iter().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let label = if ext.is_empty() { "(no extension)" } else { ext };
        if options.sizes {
            let total: u64 = entries.iter().filter_map(|e| e.size).sum();
            writeln!(
                writer,
                "--- {} ({} files, {}) ---",
                label,
                entries.len(),
                human_size(total)
            )?;
        } else {
            writeln!(writer, "--- {} ({} files) ---", label, entries.len())?;
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

fn write_json(
    files_by_ext: &HashMap<String, Vec<FileEntry>>,
    stats: &ScanStats,
    root: &Path,
    output_path: &Path,
    options: ScanOptions,
) -> io::Result<()> {
    let file = fs::File::create(output_path)?;
    let mut w = BufWriter::new(file);

    writeln!(w, "{{")?;
    writeln!(
        w,
        "  \"generator\": \"CreepDir {}\",",
        env!("CARGO_PKG_VERSION")
    )?;
    writeln!(w, "  \"root\": \"{}\",", json_escape(&root.to_string_lossy()))?;
    writeln!(w, "  \"summary\": {{")?;
    writeln!(w, "    \"files\": {},", stats.files)?;
    writeln!(w, "    \"directories\": {},", stats.dirs)?;
    writeln!(w, "    \"skipped\": {},", stats.skipped)?;
    writeln!(w, "    \"symlinks\": {},", stats.symlinks)?;
    writeln!(w, "    \"total_size\": {}", json_num_opt(options.sizes, stats.total_size))?;
    writeln!(w, "  }},")?;
    writeln!(w, "  \"extensions\": [")?;

    let extensions = sorted_extensions(files_by_ext);
    for (i, ext) in extensions.iter().enumerate() {
        let mut entries: Vec<&FileEntry> = files_by_ext[*ext].iter().collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let total: u64 = entries.iter().filter_map(|e| e.size).sum();

        writeln!(w, "    {{")?;
        writeln!(w, "      \"extension\": \"{}\",", json_escape(ext))?;
        writeln!(w, "      \"count\": {},", entries.len())?;
        writeln!(
            w,
            "      \"total_size\": {},",
            json_num_opt(options.sizes, total)
        )?;
        writeln!(w, "      \"files\": [")?;
        for (j, entry) in entries.iter().enumerate() {
            let comma = if j + 1 < entries.len() { "," } else { "" };
            let size = match entry.size {
                Some(s) => s.to_string(),
                None => "null".to_string(),
            };
            writeln!(
                w,
                "        {{ \"path\": \"{}\", \"size\": {} }}{}",
                json_escape(&entry.path.to_string_lossy()),
                size,
                comma
            )?;
        }
        writeln!(w, "      ]")?;
        let comma = if i + 1 < extensions.len() { "," } else { "" };
        writeln!(w, "    }}{}", comma)?;
    }

    writeln!(w, "  ]")?;
    writeln!(w, "}}")?;
    w.flush()
}

/// Format an optional numeric JSON value: the number when `present`, else `null`.
fn json_num_opt(present: bool, value: u64) -> String {
    if present {
        value.to_string()
    } else {
        "null".to_string()
    }
}

/// Escape a string for inclusion in a JSON string literal.
fn json_escape(s: &str) -> String {
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

/// Scan directly to a CSV file, streaming rows as files are discovered so memory
/// usage stays flat regardless of tree size.
fn scan_to_csv(
    root: &Path,
    output_path: &Path,
    options: ScanOptions,
    filter: &ScanFilter,
) -> io::Result<ScanStats> {
    let file = fs::File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "extension,path,size")?;

    let mut write_err: Option<io::Error> = None;
    let stats = walk_folder(root, options, filter, |ext, path, size| {
        if write_err.is_some() {
            return;
        }
        let size_field = size.map(|s| s.to_string()).unwrap_or_default();
        if let Err(e) = writeln!(
            writer,
            "{},{},{}",
            csv_field(&ext),
            csv_field(&path.to_string_lossy()),
            size_field
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
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
