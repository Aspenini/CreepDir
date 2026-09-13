//! CLI error type. `main` prints this with [`Display`] and exits non-zero.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Recoverable failure that should be reported to the user.
#[derive(Debug)]
pub enum Error {
    /// I/O failure with a short context label (`"writing output file"`).
    Io {
        context: &'static str,
        source: io::Error,
    },
    /// `--exclude` glob failed to parse or compile.
    Filter(String),
    /// Target path does not exist.
    NotFound { path: PathBuf, cwd: Option<PathBuf> },
    /// Target path exists but is not a directory.
    NotADirectory(PathBuf),
    /// `--select` was combined with `FOLDER` / `OUTPUT`.
    SelectWithPaths,
    /// Flags were given without a folder and without `--select`.
    MissingFolder,
}

impl Error {
    pub fn io(context: &'static str, source: io::Error) -> Self {
        Self::Io { context, source }
    }

    pub fn not_found(path: &Path) -> Self {
        let cwd = path
            .is_relative()
            .then(|| std::env::current_dir().ok())
            .flatten();
        Self::NotFound {
            path: path.to_path_buf(),
            cwd,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Filter(msg) => f.write_str(msg),
            Self::NotFound { path, cwd } => {
                write!(f, "folder '{}' does not exist", path.display())?;
                if let Some(cwd) = cwd {
                    write!(
                        f,
                        "\nCurrent directory: {}\nTried to resolve: {}",
                        cwd.display(),
                        cwd.join(path).display()
                    )?;
                }
                Ok(())
            }
            Self::NotADirectory(path) => write!(f, "'{}' is not a directory", path.display()),
            Self::SelectWithPaths => f.write_str("--select cannot be used with path arguments"),
            Self::MissingFolder => f.write_str("specify a FOLDER to scan, or use --select"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
