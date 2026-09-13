//! Configuration types shared across the scanner and output writers.

use std::collections::HashSet;
use std::sync::Arc;

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Output format for the generated catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Csv,
}

impl OutputFormat {
    /// Choose a format from the mutually-exclusive `--json` / `--csv` flags.
    #[must_use]
    pub fn from_flags(json: bool, csv: bool) -> Self {
        if json {
            Self::Json
        } else if csv {
            Self::Csv
        } else {
            Self::Text
        }
    }

    /// Default file extension for this format.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

/// Options that control how a scan is performed.
#[derive(Clone, Copy, Debug)]
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
#[derive(Debug)]
pub struct ScanFilter {
    /// Allow-list of dot-prefixed lowercase extensions (e.g. `.rs`). `None` = all.
    ext: Option<HashSet<String>>,
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
                    .filter_map(|raw| {
                        let ext = raw.trim().trim_start_matches('.').to_lowercase();
                        (!ext.is_empty()).then(|| format!(".{ext}"))
                    })
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

        Ok(Self { ext, exclude })
    }

    /// Whether a file with the given extension key should be included.
    #[must_use]
    pub fn ext_allowed(&self, ext: &str) -> bool {
        self.ext
            .as_ref()
            .is_none_or(|allowed| allowed.contains(ext))
    }

    /// The compiled exclude set, if any exclude patterns were given.
    #[must_use]
    pub fn exclude(&self) -> Option<&Arc<GlobSet>> {
        self.exclude.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_flags_prefers_json_then_csv() {
        assert_eq!(OutputFormat::from_flags(false, false), OutputFormat::Text);
        assert_eq!(OutputFormat::from_flags(true, false), OutputFormat::Json);
        assert_eq!(OutputFormat::from_flags(false, true), OutputFormat::Csv);
    }

    #[test]
    fn ext_filter_normalizes_and_dedups() {
        let filter =
            ScanFilter::new(&["RS".into(), ".txt".into(), "rs".into(), "  ".into()], &[]).unwrap();
        assert!(filter.ext_allowed(".rs"));
        assert!(filter.ext_allowed(".txt"));
        assert!(!filter.ext_allowed(".md"));
        assert!(!filter.ext_allowed(""));
    }

    #[test]
    fn empty_ext_args_allows_everything() {
        let filter = ScanFilter::new(&[], &[]).unwrap();
        assert!(filter.ext_allowed(".rs"));
        assert!(filter.ext_allowed(""));
    }

    #[test]
    fn invalid_exclude_glob_is_reported() {
        let err = ScanFilter::new(&[], &["[".into()]).unwrap_err();
        assert!(err.contains("invalid --exclude pattern"), "{err}");
    }
}
