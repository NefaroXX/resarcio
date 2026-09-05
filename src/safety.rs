use std::path::{Path, PathBuf};

use crate::error::AccordError;

/// Validate that a path component from a diff header is safe.
/// Rejects absolute paths and `..` traversal.
pub fn validate_path(path: &str) -> Result<String, AccordError> {
    if path.starts_with('/') {
        return Err(AccordError::UnsafePath(format!(
            "absolute path rejected: {}",
            path
        )));
    }
    if path.contains("..") {
        return Err(AccordError::UnsafePath(format!(
            "path traversal rejected: {}",
            path
        )));
    }
    Ok(path.to_string())
}

/// Resolve a validated path within a target directory, ensuring it doesn't escape.
pub fn resolve_within(target_dir: &Path, relative: &str) -> Result<PathBuf, AccordError> {
    let resolved = target_dir.join(relative);
    let canonical_target = target_dir.canonicalize().map_err(AccordError::Io)?;

    // For paths that don't exist yet (new files), canonicalize the parent directory.
    let canonical_resolved = if resolved.exists() {
        resolved.canonicalize().map_err(AccordError::Io)?
    } else if let Some(parent) = resolved.parent() {
        let canonical_parent = parent.canonicalize().map_err(AccordError::Io)?;
        let file_name = resolved
            .file_name()
            .ok_or_else(|| AccordError::UnsafePath(format!("invalid path: {}", relative)))?;
        canonical_parent.join(file_name)
    } else {
        resolved.canonicalize().map_err(AccordError::Io)?
    };

    if !canonical_resolved.starts_with(&canonical_target) {
        return Err(AccordError::UnsafePath(format!(
            "path escapes target directory: {}",
            relative
        )));
    }

    // Check for symlink escapes: walk up from canonical_resolved to target and ensure
    // no component is a symlink pointing outside.
    let mut check = canonical_resolved.clone();
    while check != canonical_target {
        if check
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            let link_target = std::fs::read_link(&check).map_err(AccordError::Io)?;
            let absolute = if link_target.is_absolute() {
                link_target
            } else {
                check.parent().unwrap_or(&check).join(&link_target)
            };
            let canonical_link = absolute.canonicalize().map_err(AccordError::Io)?;
            if !canonical_link.starts_with(&canonical_target) {
                return Err(AccordError::UnsafePath(format!(
                    "symlink escapes target directory: {}",
                    relative
                )));
            }
        }
        check = check.parent().unwrap_or(&check).to_path_buf();
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_path() {
        assert!(validate_path("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_dotdot_traversal() {
        assert!(validate_path("../etc/passwd").is_err());
    }

    #[test]
    fn rejects_dotdot_in_middle() {
        assert!(validate_path("foo/../../etc/passwd").is_err());
    }

    #[test]
    fn accepts_relative_path() {
        assert!(validate_path("src/main.rs").is_ok());
    }

    #[test]
    fn accepts_simple_filename() {
        assert!(validate_path("file.txt").is_ok());
    }
}
