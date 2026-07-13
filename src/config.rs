//! Configuration types shared across the scanner and output writers.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::sync::Arc;

/// Output format for the generated catalog.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Csv,
}

impl OutputFormat {
    /// Choose a format from the mutually-exclusive `--json` / `--csv` flags.
    pub fn from_flags(json: bool, csv: bool) -> Self {
        if json {
            OutputFormat::Json
        } else if csv {
            OutputFormat::Csv
        } else {
            OutputFormat::Text
        }
    }

    /// Default file extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Text => "txt",
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
        }
    }
}

/// Options that control how a scan is performed.
#[derive(Clone, Copy)]
pub struct ScanOptions {
    /// Suppress per-folder warnings for inaccessible paths.
    pub quiet: bool,
    /// Number of worker threads (`None` = one per CPU core).
    pub threads: Option<usize>,
    /// Follow symlinks/junctions (jwalk reports loops as errors).
    pub follow_symlinks: bool,
    /// Maximum recursion depth (`None` = unlimited). Depth 0 is the root itself.
    pub max_depth: Option<usize>,
    /// Collect file sizes (costs one metadata call per file).
    pub sizes: bool,
    /// Output format.
    pub format: OutputFormat,
}

/// Decides which files are included in the catalog.
pub struct ScanFilter {
    /// Allow-list of dot-prefixed lowercase extensions (e.g. `.rs`). `None` = all.
    ext: Option<Vec<String>>,
    /// Glob patterns; matching files/dirs are excluded (dirs are pruned).
    exclude: Option<Arc<GlobSet>>,
}

impl ScanFilter {
    /// Build a filter from raw `--ext` and `--exclude` CLI values.
    ///
    /// Returns `Err` with a human-readable message if an exclude glob is invalid.
    pub fn new(ext_args: &[String], exclude_args: &[String]) -> Result<Self, String> {
        let ext = if ext_args.is_empty() {
            None
        } else {
            Some(
                ext_args
                    .iter()
                    .map(|e| format!(".{}", e.trim().trim_start_matches('.').to_lowercase()))
                    .collect(),
            )
        };

        let exclude = if exclude_args.is_empty() {
            None
        } else {
            let mut builder = GlobSetBuilder::new();
            for pattern in exclude_args {
                let glob = Glob::new(pattern)
                    .map_err(|e| format!("invalid --exclude pattern '{pattern}': {e}"))?;
                builder.add(glob);
            }
            let set = builder
                .build()
                .map_err(|e| format!("failed to build exclude patterns: {e}"))?;
            Some(Arc::new(set))
        };

        Ok(ScanFilter { ext, exclude })
    }

    /// Whether a file with the given extension key should be included.
    pub fn ext_allowed(&self, ext: &str) -> bool {
        match &self.ext {
            Some(allowed) => allowed.iter().any(|e| e == ext),
            None => true,
        }
    }

    /// The compiled exclude set, if any exclude patterns were given.
    pub fn exclude(&self) -> Option<&Arc<GlobSet>> {
        self.exclude.as_ref()
    }
}
