use std::fmt;

/// Structured error type for resarcio operations.
#[derive(Debug)]
pub enum ResarcioError {
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

impl fmt::Display for ResarcioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResarcioError::Parse(msg) => write!(f, "parse error: {}", msg),
            ResarcioError::ContextMismatch {
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
            ResarcioError::UnsafePath(p) => write!(f, "unsafe path rejected: {}", p),
            ResarcioError::Io(e) => write!(f, "I/O error: {}", e),
            ResarcioError::FileNotFound(p) => write!(f, "file not found: {}", p),
            ResarcioError::IsADirectory(p) => write!(f, "expected file but found directory: {}", p),
            ResarcioError::FileAlreadyExists(p) => write!(f, "file already exists: {}", p),
            ResarcioError::DeleteTargetNotFound(p) => {
                write!(f, "cannot delete - file not found: {}", p)
            }
            ResarcioError::EmptyDiff => write!(f, "empty diff - no file patches found"),
        }
    }
}

impl std::error::Error for ResarcioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResarcioError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ResarcioError {
    fn from(e: std::io::Error) -> Self {
        ResarcioError::Io(e)
    }
}
