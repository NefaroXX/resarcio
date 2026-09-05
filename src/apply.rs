use std::path::Path;

use crate::error::ResarcioError;
use crate::parse::{FilePatch, Hunk, HunkLine};

/// Options controlling how patches are applied.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApplyOptions {
    /// Show what would change without writing files.
    pub dry_run: bool,
    /// Verify the patch applies cleanly (read-only, but validates).
    pub check: bool,
    /// Apply the patch in reverse (undo it).
    pub reverse: bool,
    /// Ignore whitespace differences when matching context lines.
    pub ignore_whitespace: bool,
    /// Insert conflict markers on hunk mismatch instead of failing.
    pub conflict_markers: bool,
    /// Write failed hunks to `.rej` sidecar files.
    pub write_rej: bool,
}

impl ApplyOptions {
    /// Standard apply (no special modes).
    #[allow(dead_code)]
    pub fn apply() -> Self {
        Self {
            dry_run: false,
            check: false,
            reverse: false,
            ignore_whitespace: false,
            conflict_markers: false,
            write_rej: false,
        }
    }

    /// Dry-run mode (read-only).
    #[allow(dead_code)]
    pub fn dry_run() -> Self {
        Self {
            dry_run: true,
            check: false,
            reverse: false,
            ignore_whitespace: false,
            conflict_markers: false,
            write_rej: false,
        }
    }
}

/// Outcome of applying a single hunk.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum HunkOutcome {
    /// Hunk was applied successfully.
    Applied,
    /// Hunk context did not match — conflict markers may be inserted.
    Mismatch {
        expected_line: String,
        found_line: String,
        file_line: usize,
    },
}

/// Result of applying all hunks for a single file.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileResult {
    /// The file path that was patched.
    pub path: String,
    /// Number of hunks applied successfully.
    pub hunks_applied: usize,
    /// Number of hunks that failed to match.
    pub hunks_failed: usize,
    /// Hunks that failed (for `.rej` file generation).
    pub rejected_hunks: Vec<Hunk>,
    /// Whether this file was created (new file patch).
    pub is_new: bool,
    /// Whether this file was deleted.
    pub is_deleted: bool,
    /// Number of lines added.
    pub insertions: usize,
    /// Number of lines removed.
    pub deletions: usize,
}

/// Aggregate report of applying a full diff.
#[derive(Debug, Clone)]
pub struct ApplyReport {
    /// Per-file results.
    pub files: Vec<FileResult>,
}

impl ApplyReport {
    /// Total insertions across all files.
    pub fn total_insertions(&self) -> usize {
        self.files.iter().map(|f| f.insertions).sum()
    }

    /// Total deletions across all files.
    pub fn total_deletions(&self) -> usize {
        self.files.iter().map(|f| f.deletions).sum()
    }

    /// Number of files with at least one change.
    pub fn files_changed(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.hunks_applied > 0 || f.is_new || f.is_deleted)
            .count()
    }
}

/// Apply a file patch to the target directory.
pub fn apply_file_patch(
    target_dir: &Path,
    patch: &FilePatch,
    options: &ApplyOptions,
) -> Result<FileResult, ResarcioError> {
    if patch.is_new_file {
        let target = target_dir.join(&patch.new_path);
        return apply_new_file(&target, patch, options);
    }

    if patch.is_deleted {
        let target = target_dir.join(&patch.old_path);
        return apply_delete_file(&target, patch, options);
    }

    let target = target_dir.join(&patch.new_path);

    // Read existing file
    let content = std::fs::read_to_string(&target).map_err(ResarcioError::Io)?;
    let old_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let had_newline = content.ends_with('\n');

    // Apply hunks sequentially
    let mut new_lines = old_lines.clone();
    let mut line_offset: isize = 0;
    let mut hunks_applied: usize = 0;
    let mut hunks_failed: usize = 0;
    let mut rejected_hunks: Vec<Hunk> = Vec::new();
    let mut insertions: usize = 0;
    let mut deletions: usize = 0;

    for hunk in &patch.hunks {
        let outcome = apply_hunk(
            &mut new_lines,
            hunk,
            &mut line_offset,
            &patch.new_path,
            options,
        )?;

        match outcome {
            HunkOutcome::Applied => {
                hunks_applied += 1;
                insertions += hunk
                    .lines
                    .iter()
                    .filter(|l| matches!(l, HunkLine::Added(_)))
                    .count();
                deletions += hunk
                    .lines
                    .iter()
                    .filter(|l| matches!(l, HunkLine::Removed(_)))
                    .count();
            }
            HunkOutcome::Mismatch { .. } => {
                hunks_failed += 1;
                rejected_hunks.push(hunk.clone());
            }
        }
    }

    if !options.dry_run {
        // Build output, respecting NoNewlineMarker on the last applied hunk
        let mut output = new_lines.join("\n");
        let has_no_newline_marker = patch
            .hunks
            .iter()
            .rfind(|h| !rejected_hunks.iter().any(|r| std::ptr::eq(*h, r)))
            .map(|h| h.lines.last() == Some(&HunkLine::NoNewlineMarker))
            .unwrap_or(false);

        if has_no_newline_marker {
            // No trailing newline
        } else if had_newline || !old_lines.is_empty() {
            output.push('\n');
        }

        std::fs::write(&target, output).map_err(ResarcioError::Io)?;
        println!("applied: {}", patch.new_path);

        // Write .rej file if there are rejected hunks
        if options.write_rej && !rejected_hunks.is_empty() {
            let rej_path = target_dir.join(format!("{}.rej", patch.new_path));
            write_rej_file(&rej_path, &patch.new_path, &rejected_hunks)?;
            println!("rejected: {}.rej", patch.new_path);
        }
    } else {
        println!("would apply: {}", patch.new_path);
    }

    Ok(FileResult {
        path: patch.new_path.clone(),
        hunks_applied,
        hunks_failed,
        rejected_hunks,
        is_new: false,
        is_deleted: false,
        insertions,
        deletions,
    })
}

/// Apply a new-file patch.
fn apply_new_file(
    target: &Path,
    patch: &FilePatch,
    options: &ApplyOptions,
) -> Result<FileResult, ResarcioError> {
    if target.exists() {
        return Err(ResarcioError::FileAlreadyExists(patch.new_path.clone()));
    }

    let content = extract_added_content(patch)?;
    let has_no_newline = patch
        .hunks
        .last()
        .map(|h| h.lines.last() == Some(&HunkLine::NoNewlineMarker))
        .unwrap_or(false);

    let insertions = content.lines().count();

    if !options.dry_run {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(ResarcioError::Io)?;
        }
        if has_no_newline {
            std::fs::write(target, &content).map_err(ResarcioError::Io)?;
        } else {
            let mut output = content;
            output.push('\n');
            std::fs::write(target, output).map_err(ResarcioError::Io)?;
        }
        println!("created: {}", patch.new_path);
    } else {
        println!("would create: {}", patch.new_path);
    }

    Ok(FileResult {
        path: patch.new_path.clone(),
        hunks_applied: patch.hunks.len(),
        hunks_failed: 0,
        rejected_hunks: Vec::new(),
        is_new: true,
        is_deleted: false,
        insertions,
        deletions: 0,
    })
}

/// Apply a file deletion patch.
fn apply_delete_file(
    target: &Path,
    patch: &FilePatch,
    options: &ApplyOptions,
) -> Result<FileResult, ResarcioError> {
    if !target.exists() {
        return Err(ResarcioError::DeleteTargetNotFound(patch.old_path.clone()));
    }

    let deletions = if target.exists() && !options.dry_run {
        std::fs::read_to_string(target)
            .map(|c| c.lines().count())
            .unwrap_or(0)
    } else {
        0
    };

    if !options.dry_run {
        std::fs::remove_file(target).map_err(ResarcioError::Io)?;
        println!("deleted: {}", patch.old_path);
    } else {
        println!("would delete: {}", patch.old_path);
    }

    Ok(FileResult {
        path: patch.old_path.clone(),
        hunks_applied: patch.hunks.len(),
        hunks_failed: 0,
        rejected_hunks: Vec::new(),
        is_new: false,
        is_deleted: true,
        insertions: 0,
        deletions,
    })
}

/// Extract the content from Added lines in a patch.
fn extract_added_content(patch: &FilePatch) -> Result<String, ResarcioError> {
    let mut lines = Vec::new();
    for hunk in &patch.hunks {
        for line in &hunk.lines {
            match line {
                HunkLine::Added(content) => lines.push(content.clone()),
                HunkLine::NoNewlineMarker => {}
                _ => {}
            }
        }
    }
    Ok(lines.join("\n"))
}

/// Normalize whitespace for comparison: collapse runs to single space, trim.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Apply a single hunk to the new_lines vector.
fn apply_hunk(
    new_lines: &mut Vec<String>,
    hunk: &Hunk,
    line_offset: &mut isize,
    file_path: &str,
    options: &ApplyOptions,
) -> Result<HunkOutcome, ResarcioError> {
    let hunk_old_start = hunk.old_start;
    let old_count = hunk_old_count(hunk);

    let search_start = if hunk_old_start == 0 {
        0
    } else {
        (hunk_old_start as isize + *line_offset - 1).max(0) as usize
    };

    // Pure insertion: no existing lines to match
    let match_pos = if old_count == 0 {
        if hunk_old_start == 0 {
            0
        } else {
            (hunk_old_start as isize + *line_offset).max(0) as usize
        }
    } else {
        // Compute diagnostic info before the match attempt
        let first_ctx = hunk.lines.iter().find_map(|l| match l {
            HunkLine::Context(s) => Some(s.as_str()),
            _ => None,
        });
        let found_at_start = new_lines
            .get(search_start)
            .map(|s| s.as_str())
            .unwrap_or("<end of file>")
            .to_string();

        match find_hunk_match(
            new_lines,
            &hunk.lines,
            search_start,
            options.ignore_whitespace,
        ) {
            Some(pos) => pos,
            None => {
                // In dry-run (but not check), don't fail — we're not modifying anything
                if options.dry_run && !options.check {
                    return Ok(HunkOutcome::Applied);
                }
                // --check takes precedence: validate cleanly or fail
                if !options.check && options.conflict_markers {
                    // Insert conflict markers at the expected position
                    let insert_at = search_start.min(new_lines.len());
                    let mut marker_lines = vec![
                        "<<<<<<< PATCH".to_string(),
                        first_ctx.unwrap_or("<empty>").to_string(),
                        "=======".to_string(),
                    ];
                    // Add the lines that were supposed to be there from the hunk
                    for line in &hunk.lines {
                        match line {
                            HunkLine::Context(s) | HunkLine::Removed(s) => {
                                marker_lines.push(s.clone());
                            }
                            HunkLine::Added(_) | HunkLine::NoNewlineMarker => {}
                        }
                    }
                    marker_lines.push(">>>>>>> CURRENT".to_string());

                    new_lines.splice(insert_at..insert_at, marker_lines);

                    return Ok(HunkOutcome::Mismatch {
                        expected_line: first_ctx.unwrap_or("<empty>").to_string(),
                        found_line: found_at_start,
                        file_line: search_start + 1,
                    });
                }

                return Err(ResarcioError::ContextMismatch {
                    file: file_path.to_string(),
                    hunk_line: hunk.old_start,
                    expected: first_ctx.unwrap_or("<empty>").to_string(),
                    found: found_at_start,
                    file_line: search_start + 1,
                });
            }
        }
    };

    if options.dry_run {
        return Ok(HunkOutcome::Applied);
    }

    // Build replacement lines from the hunk
    let mut replacement: Vec<String> = Vec::new();
    for line in &hunk.lines {
        match line {
            HunkLine::Context(s) | HunkLine::Added(s) => replacement.push(s.clone()),
            HunkLine::Removed(_) => {}
            HunkLine::NoNewlineMarker => {}
        }
    }

    // Replace the old lines with the new lines
    let replace_end = (match_pos + old_count).min(new_lines.len());
    new_lines.splice(match_pos..replace_end, replacement);

    // Adjust offset for subsequent hunks
    let removed_count = old_count;
    let added_count: usize = hunk
        .lines
        .iter()
        .filter(|l| matches!(l, HunkLine::Context(_) | HunkLine::Added(_)))
        .count();
    *line_offset += added_count as isize - removed_count as isize;

    Ok(HunkOutcome::Applied)
}

/// Count how many old-file lines a hunk consumes.
fn hunk_old_count(hunk: &Hunk) -> usize {
    hunk.lines
        .iter()
        .filter(|l| matches!(l, HunkLine::Context(_) | HunkLine::Removed(_)))
        .count()
}

/// Find where a hunk matches in the current file content.
fn find_hunk_match(
    lines: &[String],
    hunk_lines: &[HunkLine],
    start: usize,
    ignore_whitespace: bool,
) -> Option<usize> {
    if hunk_lines.is_empty() {
        return Some(start);
    }

    if lines.is_empty() {
        return None;
    }

    // Find first Context line to anchor the search.
    let first_ctx_offset = hunk_lines
        .iter()
        .position(|l| matches!(l, HunkLine::Context(_)));

    // If no context lines, use Removed lines to anchor instead.
    let anchor_offset = first_ctx_offset.or_else(|| {
        hunk_lines
            .iter()
            .position(|l| matches!(l, HunkLine::Removed(_)))
    });

    let anchor_offset = match anchor_offset {
        Some(o) => o,
        // Pure Added hunk — match at start.
        None => return Some(start),
    };

    let anchor_text = match &hunk_lines[anchor_offset] {
        HunkLine::Context(s) | HunkLine::Removed(s) => s.as_str(),
        _ => unreachable!(),
    };

    let end_limit = lines.len().saturating_sub(1);

    'outer: for i in start..=end_limit {
        let line_matches = if ignore_whitespace {
            normalize_whitespace(&lines[i]) == normalize_whitespace(anchor_text)
        } else {
            lines[i] == anchor_text
        };

        if !line_matches {
            continue;
        }

        // Candidate: verify all hunk lines forward from anchor
        let mut idx = i;
        for hunk_line in &hunk_lines[anchor_offset..] {
            match hunk_line {
                HunkLine::Context(s) | HunkLine::Removed(s) => {
                    if idx >= lines.len() {
                        continue 'outer;
                    }
                    let matches = if ignore_whitespace {
                        normalize_whitespace(&lines[idx]) == normalize_whitespace(s)
                    } else {
                        lines[idx] == *s
                    };
                    if !matches {
                        continue 'outer;
                    }
                    idx += 1;
                }
                HunkLine::Added(_) | HunkLine::NoNewlineMarker => {}
            }
        }

        // Verify hunk lines BEFORE anchor_offset
        let mut idx_before = i;
        for hunk_line in hunk_lines[..anchor_offset].iter().rev() {
            match hunk_line {
                HunkLine::Context(s) | HunkLine::Removed(s) => {
                    if idx_before == 0 {
                        continue 'outer;
                    }
                    let matches = if ignore_whitespace {
                        normalize_whitespace(&lines[idx_before - 1]) == normalize_whitespace(s)
                    } else {
                        lines[idx_before - 1] == *s
                    };
                    if !matches {
                        continue 'outer;
                    }
                    idx_before -= 1;
                }
                HunkLine::Added(_) | HunkLine::NoNewlineMarker => {}
            }
        }

        return Some(idx_before);
    }

    None
}

/// Write rejected hunks to a `.rej` file.
fn write_rej_file(path: &Path, _file_path: &str, hunks: &[Hunk]) -> Result<(), ResarcioError> {
    let mut content = String::new();
    for hunk in hunks {
        content.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_count, hunk.new_start, hunk.new_count
        ));
        for line in &hunk.lines {
            match line {
                HunkLine::Context(s) => content.push_str(&format!(" {}\n", s)),
                HunkLine::Added(s) => content.push_str(&format!("+{}\n", s)),
                HunkLine::Removed(s) => content.push_str(&format!("-{}\n", s)),
                HunkLine::NoNewlineMarker => content.push_str("\\ No newline at end of file\n"),
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ResarcioError::Io)?;
    }
    std::fs::write(path, content).map_err(ResarcioError::Io)?;

    Ok(())
}

/// Format stat output (like `git diff --stat`).
pub fn format_stat(report: &ApplyReport) -> String {
    let mut out = String::new();

    for file in &report.files {
        let changes = file.insertions + file.deletions;
        if changes == 0 {
            continue;
        }
        let path = &file.path;
        let bar = if file.insertions > 0 && file.deletions > 0 {
            format!(
                "{}+{}",
                "+".repeat(file.insertions.min(5)),
                "-".repeat(file.deletions.min(5))
            )
        } else if file.insertions > 0 {
            "+".repeat(file.insertions.min(7))
        } else {
            "-".repeat(file.deletions.min(7))
        };
        out.push_str(&format!("{path} | {changes} {bar}\n"));
    }

    let total_ins = report.total_insertions();
    let total_del = report.total_deletions();
    let files = report.files_changed();
    out.push_str(&format!(
        "{} file{} changed, {} insertion{}(+), {} deletion{}(-)\n",
        files,
        if files == 1 { "" } else { "s" },
        total_ins,
        if total_ins == 1 { "" } else { "s" },
        total_del,
        if total_del == 1 { "" } else { "s" },
    ));

    out
}

/// Format JSON output.
pub fn format_json(report: &ApplyReport) -> String {
    let mut out = String::from("{\"files\":[");

    for (i, file) in report.files.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"path\":\"{}\",\"hunks_applied\":{},\"hunks_failed\":{},\"insertions\":{},\"deletions\":{}}}",
            escape_json(&file.path),
            file.hunks_applied,
            file.hunks_failed,
            file.insertions,
            file.deletions,
        ));
    }

    out.push_str(&format!(
        "],\"summary\":{{\"files_changed\":{},\"insertions\":{},\"deletions\":{}}}}}",
        report.files_changed(),
        report.total_insertions(),
        report.total_deletions(),
    ));

    out
}

/// Escape a string for JSON output.
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello  world"), "hello world");
        assert_eq!(normalize_whitespace("  hello\tworld  "), "hello world");
        assert_eq!(normalize_whitespace("no_change"), "no_change");
    }

    #[test]
    fn test_hunk_old_count() {
        let hunk = Hunk {
            old_start: 1,
            old_count: 3,
            new_start: 1,
            new_count: 2,
            lines: vec![
                HunkLine::Context("a".into()),
                HunkLine::Removed("b".into()),
                HunkLine::Added("c".into()),
            ],
        };
        assert_eq!(hunk_old_count(&hunk), 2); // context + removed
    }

    #[test]
    fn test_format_json() {
        let report = ApplyReport {
            files: vec![FileResult {
                path: "foo.rs".into(),
                hunks_applied: 2,
                hunks_failed: 0,
                rejected_hunks: Vec::new(),
                is_new: false,
                is_deleted: false,
                insertions: 5,
                deletions: 3,
            }],
        };
        let json = format_json(&report);
        assert!(json.contains("\"path\":\"foo.rs\""));
        assert!(json.contains("\"insertions\":5"));
        assert!(json.contains("\"files_changed\":1"));
    }

    #[test]
    fn test_format_stat() {
        let report = ApplyReport {
            files: vec![FileResult {
                path: "foo.rs".into(),
                hunks_applied: 1,
                hunks_failed: 0,
                rejected_hunks: Vec::new(),
                is_new: false,
                is_deleted: false,
                insertions: 3,
                deletions: 2,
            }],
        };
        let stat = format_stat(&report);
        assert!(stat.contains("foo.rs"));
        assert!(stat.contains("5"));
        assert!(stat.contains("insertion"));
        assert!(stat.contains("deletion"));
    }
}
