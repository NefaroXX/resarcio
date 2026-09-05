use std::path::Path;

use crate::error::ResarcioError;
use crate::parse::{FilePatch, Hunk, HunkLine};

/// Apply a file patch to the target directory.
pub fn apply_file_patch(
    target_dir: &Path,
    patch: &FilePatch,
    dry_run: bool,
) -> Result<(), ResarcioError> {
    if patch.is_new_file {
        let target = target_dir.join(&patch.new_path);
        return apply_new_file(&target, patch, dry_run);
    }

    if patch.is_deleted {
        let target = target_dir.join(&patch.old_path);
        return apply_delete_file(&target, patch, dry_run);
    }

    let target = target_dir.join(&patch.new_path);

    // Read existing file
    let content = std::fs::read_to_string(&target).map_err(ResarcioError::Io)?;
    let old_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    // Track whether the file ends with a newline
    let had_newline = content.ends_with('\n');

    // Apply hunks sequentially
    let mut new_lines = old_lines.clone();
    let mut line_offset: isize = 0;

    for hunk in &patch.hunks {
        apply_hunk(
            &mut new_lines,
            hunk,
            &mut line_offset,
            &patch.new_path,
            dry_run,
        )?;
    }

    if !dry_run {
        // Build output, respecting NoNewlineMarker on the last line
        let mut output = new_lines.join("\n");
        let last_hunk = patch.hunks.last();
        let has_no_newline_marker = last_hunk
            .map(|h| h.lines.last() == Some(&HunkLine::NoNewlineMarker))
            .unwrap_or(false);

        if has_no_newline_marker {
            // No trailing newline
        } else if had_newline || !old_lines.is_empty() {
            output.push('\n');
        }

        std::fs::write(&target, output).map_err(ResarcioError::Io)?;
        println!("applied: {}", patch.new_path);
    } else {
        println!("would apply: {}", patch.new_path);
    }

    Ok(())
}

/// Apply a new-file patch.
fn apply_new_file(target: &Path, patch: &FilePatch, dry_run: bool) -> Result<(), ResarcioError> {
    if target.exists() {
        return Err(ResarcioError::FileAlreadyExists(patch.new_path.clone()));
    }

    let content = extract_added_content(patch)?;
    let has_no_newline = patch
        .hunks
        .last()
        .map(|h| h.lines.last() == Some(&HunkLine::NoNewlineMarker))
        .unwrap_or(false);

    if !dry_run {
        // Create parent directories if needed
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

    Ok(())
}

/// Apply a file deletion patch.
fn apply_delete_file(target: &Path, patch: &FilePatch, dry_run: bool) -> Result<(), ResarcioError> {
    if !target.exists() {
        return Err(ResarcioError::DeleteTargetNotFound(patch.old_path.clone()));
    }

    if !dry_run {
        std::fs::remove_file(target).map_err(ResarcioError::Io)?;
        println!("deleted: {}", patch.old_path);
    } else {
        println!("would delete: {}", patch.old_path);
    }

    Ok(())
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

/// Apply a single hunk to the new_lines vector.
fn apply_hunk(
    new_lines: &mut Vec<String>,
    hunk: &Hunk,
    line_offset: &mut isize,
    file_path: &str,
    dry_run: bool,
) -> Result<(), ResarcioError> {
    let hunk_old_start = hunk.old_start; // 1-based
    let old_count = hunk_old_count(hunk);

    // Compute where to start searching in new_lines.
    let search_start = if hunk_old_start == 0 {
        0
    } else {
        (hunk_old_start as isize + *line_offset - 1).max(0) as usize
    };

    // Pure insertion (old_count == 0): no existing lines to match, just insert
    // at the computed position. For pure insertions, old_start is the 1-based
    // line number AFTER which to insert, so the 0-indexed position is old_start
    // itself (not old_start - 1 as for replacements).
    let match_pos = if old_count == 0 {
        if hunk_old_start == 0 {
            0
        } else {
            (hunk_old_start as isize + *line_offset).max(0) as usize
        }
    } else {
        // Search new_lines for where the context matches.
        match find_hunk_match(new_lines, &hunk.lines, search_start) {
            Some(pos) => pos,
            None => {
                let first_ctx = hunk.lines.iter().find_map(|l| match l {
                    HunkLine::Context(s) => Some(s.as_str()),
                    _ => None,
                });
                return Err(ResarcioError::ContextMismatch {
                    file: file_path.to_string(),
                    hunk_line: hunk.old_start,
                    expected: first_ctx.unwrap_or("<empty>").to_string(),
                    found: new_lines
                        .get(search_start)
                        .map(|s| s.as_str())
                        .unwrap_or("<end of file>")
                        .to_string(),
                    file_line: search_start + 1,
                });
            }
        }
    };

    if dry_run {
        return Ok(());
    }

    // Build replacement lines from the hunk (owned Strings to avoid lifetime issues)
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

    Ok(())
}

/// Count how many old-file lines a hunk consumes.
fn hunk_old_count(hunk: &Hunk) -> usize {
    hunk.lines
        .iter()
        .filter(|l| matches!(l, HunkLine::Context(_) | HunkLine::Removed(_)))
        .count()
}
/// Find where a hunk matches in the current file content.
///
/// Searches `lines` for a position where all Context and Removed hunk lines
/// match exactly. This operates on the current state of the file (after prior
/// hunks have been applied), so context lines must match the modified content.
fn find_hunk_match(lines: &[String], hunk_lines: &[HunkLine], start: usize) -> Option<usize> {
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
        if lines[i] != anchor_text {
            continue;
        }

        // Candidate: lines[i] matches the anchor line.
        // Walk the hunk from anchor_offset forward, advancing idx in parallel.
        let mut idx = i;
        for hunk_line in &hunk_lines[anchor_offset..] {
            match hunk_line {
                HunkLine::Context(s) | HunkLine::Removed(s) => {
                    if idx >= lines.len() || lines[idx] != *s {
                        continue 'outer;
                    }
                    idx += 1;
                }
                HunkLine::Added(_) | HunkLine::NoNewlineMarker => {}
            }
        }

        // Also verify any hunk lines BEFORE anchor_offset.
        // Walk backwards from anchor_offset.
        let mut idx_before = i;
        for hunk_line in hunk_lines[..anchor_offset].iter().rev() {
            match hunk_line {
                HunkLine::Context(s) | HunkLine::Removed(s) => {
                    if idx_before == 0 || lines[idx_before - 1] != *s {
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
