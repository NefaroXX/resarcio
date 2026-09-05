use crate::error::AccordError;

/// A single line in a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkLine {
    Context(String),
    Added(String),
    Removed(String),
    NoNewlineMarker,
}

/// A hunk within a file patch.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields are read by apply.rs; bin-crate analysis can't see that
pub struct Hunk {
    /// 1-based line number in the new file where this hunk starts.
    pub new_start: usize,
    /// Number of new-file lines (context + added) in this hunk.
    pub new_count: usize,
    /// 1-based line number in the old file where this hunk starts.
    pub old_start: usize,
    /// Number of old-file lines (context + removed) in this hunk.
    pub old_count: usize,
    /// The lines in this hunk.
    pub lines: Vec<HunkLine>,
}

/// A patch for a single file.
#[derive(Debug, Clone)]
pub struct FilePatch {
    /// Original file path (from `--- a/...` header).
    pub old_path: String,
    /// New file path (from `+++ b/...` header).
    pub new_path: String,
    /// Whether this file is new (no `---` line, or `--- /dev/null`).
    pub is_new_file: bool,
    /// Whether this file is deleted (no `+++` line, or `+++ /dev/null`).
    pub is_deleted: bool,
    /// The hunks to apply.
    pub hunks: Vec<Hunk>,
}

/// A complete diff containing one or more file patches.
#[derive(Debug)]
pub struct Diff {
    pub files: Vec<FilePatch>,
}

/// Strip the `a/` or `b/` prefix from a diff path.
fn strip_diff_prefix(path: &str) -> &str {
    if let Some(stripped) = path.strip_prefix("a/") {
        stripped
    } else if let Some(stripped) = path.strip_prefix("b/") {
        stripped
    } else {
        path
    }
}

/// Parse a `@@ -old_start,old_count +new_start,new_count @@` header.
fn parse_hunk_header(line: &str) -> Result<(usize, usize, usize, usize), AccordError> {
    let line = line.trim_start_matches('@');
    let line = line.trim_end_matches('@');
    let line = line.trim();

    // Format: "-old_start,old_count +new_start,new_count"
    // Old part
    let (old_part, rest) = line
        .split_once(' ')
        .ok_or_else(|| AccordError::Parse(format!("invalid hunk header: {}", line)))?;

    let (old_start, old_count) = parse_range(old_part)
        .map_err(|_| AccordError::Parse(format!("invalid old range: {}", old_part)))?;

    // New part (may have trailing space/context after @@)
    let new_part = rest
        .split_whitespace()
        .next()
        .ok_or_else(|| AccordError::Parse(format!("invalid hunk header: {}", line)))?;

    let (new_start, new_count) = parse_range(new_part)
        .map_err(|_| AccordError::Parse(format!("invalid new range: {}", new_part)))?;

    Ok((old_start, old_count, new_start, new_count))
}

/// Parse a range like "123" or "123,45" into (start, count).
/// Strips a leading `+` or `-` diff marker before parsing (unified diff headers
/// use `-old_start,old_count +new_start,new_count`).
fn parse_range(s: &str) -> Result<(usize, usize), ()> {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    if let Some((start_str, count_str)) = s.split_once(',') {
        let start: usize = start_str.parse().map_err(|_| ())?;
        let count: usize = count_str.parse().map_err(|_| ())?;
        Ok((start, count))
    } else {
        let start: usize = s.parse().map_err(|_| ())?;
        // A single number means 1 line.
        Ok((start, 1))
    }
}

/// Parse a unified diff string into a Diff structure.
pub fn parse_diff(input: &str) -> Result<Diff, AccordError> {
    let lines: Vec<&str> = input.lines().collect();
    let mut files = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Skip non-header lines
        if !lines[i].starts_with("diff ") && !lines[i].starts_with("--- ") {
            i += 1;
            continue;
        }

        // Skip optional "diff --git a/... b/..." line
        if lines[i].starts_with("diff ") {
            i += 1;
        }

        // Parse --- line
        if i >= lines.len() || !lines[i].starts_with("--- ") {
            return Err(AccordError::Parse(format!(
                "expected `---` header at line {}",
                i + 1
            )));
        }
        let old_path = strip_diff_prefix(lines[i][4..].trim()).to_string();
        let is_new_file = old_path == "/dev/null" || old_path == "dev/null";
        i += 1;

        // Parse +++ line
        if i >= lines.len() || !lines[i].starts_with("+++ ") {
            return Err(AccordError::Parse(format!(
                "expected `+++` header at line {}",
                i + 1
            )));
        }
        let new_path = strip_diff_prefix(lines[i][4..].trim()).to_string();
        let is_deleted = new_path == "/dev/null" || new_path == "dev/null";
        i += 1;

        // Parse hunks
        let mut hunks = Vec::new();
        while i < lines.len() && lines[i].starts_with("@@") {
            let (old_start, old_count, new_start, new_count) = parse_hunk_header(lines[i])?;
            i += 1;

            let mut hunk_lines = Vec::new();
            let mut expected_old = old_count;
            let mut expected_new = new_count;

            while i < lines.len() {
                let line = lines[i];
                if line.starts_with("@@") || line.starts_with("diff ") || line.starts_with("--- ") {
                    break;
                }

                if line == "\\ No newline at end of file" {
                    hunk_lines.push(HunkLine::NoNewlineMarker);
                    i += 1;
                    continue;
                }

                if let Some(content) = line.strip_prefix(' ') {
                    hunk_lines.push(HunkLine::Context(content.to_string()));
                    expected_old = expected_old.saturating_sub(1);
                    expected_new = expected_new.saturating_sub(1);
                } else if let Some(content) = line.strip_prefix('-') {
                    hunk_lines.push(HunkLine::Removed(content.to_string()));
                    expected_old = expected_old.saturating_sub(1);
                } else if let Some(content) = line.strip_prefix('+') {
                    hunk_lines.push(HunkLine::Added(content.to_string()));
                    expected_new = expected_new.saturating_sub(1);
                } else {
                    // Treat bare lines as context (some diffs omit the leading space)
                    hunk_lines.push(HunkLine::Context(line.to_string()));
                    expected_old = expected_old.saturating_sub(1);
                    expected_new = expected_new.saturating_sub(1);
                }
                i += 1;
            }

            hunks.push(Hunk {
                new_start,
                new_count,
                old_start,
                old_count,
                lines: hunk_lines,
            });
        }

        files.push(FilePatch {
            old_path,
            new_path,
            is_new_file,
            is_deleted,
            hunks,
        });
    }

    if files.is_empty() {
        return Err(AccordError::EmptyDiff);
    }

    Ok(Diff { files })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_file_single_hunk() {
        let input = "--- a/foo.txt\n+++ b/foo.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n";
        let diff = parse_diff(input).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].new_path, "foo.txt");
        assert_eq!(diff.files[0].hunks.len(), 1);
    }

    #[test]
    fn parses_multi_file_diff() {
        let input = "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n--- b/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-old2\n+new2\n";
        let diff = parse_diff(input).unwrap();
        assert_eq!(diff.files.len(), 2);
    }

    #[test]
    fn rejects_absolute_path() {
        let input = "--- /etc/passwd\n+++ /etc/passwd\n@@ -1 +1 @@\n-old\n+new\n";
        // Should parse fine, but safety check should catch it later
        let diff = parse_diff(input).unwrap();
        assert_eq!(diff.files[0].old_path, "/etc/passwd");
    }

    #[test]
    fn parses_no_newline_marker() {
        let input =
            "--- a/foo.txt\n+++ b/foo.txt\n@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file\n";
        let diff = parse_diff(input).unwrap();
        assert_eq!(diff.files[0].hunks[0].lines.len(), 3);
        assert_eq!(diff.files[0].hunks[0].lines[2], HunkLine::NoNewlineMarker);
    }

    #[test]
    fn parses_new_file() {
        let input = "--- /dev/null\n+++ b/new.txt\n@@ -0 +1 @@\n+hello\n";
        let diff = parse_diff(input).unwrap();
        assert!(diff.files[0].is_new_file);
    }

    #[test]
    fn parses_deleted_file() {
        let input = "--- a/old.txt\n+++ /dev/null\n@@ -1 +0 @@\n-goodbye\n";
        let diff = parse_diff(input).unwrap();
        assert!(diff.files[0].is_deleted);
    }

    #[test]
    fn empty_diff_returns_error() {
        let input = "nothing here";
        assert!(parse_diff(input).is_err());
    }
}
