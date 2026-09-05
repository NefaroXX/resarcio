use std::fmt;

/// Structured error type for accord operations.
#[derive(Debug)]
pub enum AccordError {
    /// The diff could not be parsed.
    Parse(String),
    /// A context line in a hunk did not match the file content.
    ContextMismatch {
        file: String,
        hunk_line: usize,
        expected: String,
        found: String,
        file_line: usize,
    },
    /// A path in the diff headers is unsafe (absolute, traversal, symlink escape).
    UnsafePath(String),
    /// The target file could not be read or written.
    Io(std::io::Error),
    /// The diff references a file that does not exist.
    FileNotFound(String),
    /// The diff references a file that is actually a directory.
    IsADirectory(String),
    /// A new-file patch targets a path that already exists.
    FileAlreadyExists(String),
    /// The diff attempts to delete a file that does not exist.
    DeleteTargetNotFound(String),
    /// No files were found in the diff.
    EmptyDiff,
}

impl fmt::Display for AccordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccordError::Parse(msg) => write!(f, "parse error: {}", msg),
            AccordError::ContextMismatch {
                file,
                hunk_line,
                expected,
                found,
                file_line,
            } => write!(
                f,
                "context mismatch in `{}` at hunk line {} (file line {}):\n  expected: {:?}\n  found:    {:?}",
                file, hunk_line, file_line, expected, found
            ),
            AccordError::UnsafePath(p) => write!(f, "unsafe path rejected: {}", p),
            AccordError::Io(e) => write!(f, "I/O error: {}", e),
            AccordError::FileNotFound(p) => write!(f, "file not found: {}", p),
            AccordError::IsADirectory(p) => write!(f, "expected file but found directory: {}", p),
            AccordError::FileAlreadyExists(p) => write!(f, "file already exists: {}", p),
            AccordError::DeleteTargetNotFound(p) => {
                write!(f, "cannot delete - file not found: {}", p)
            }
            AccordError::EmptyDiff => write!(f, "empty diff - no file patches found"),
        }
    }
}

impl std::error::Error for AccordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AccordError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for AccordError {
    fn from(e: std::io::Error) -> Self {
        AccordError::Io(e)
    }
}
