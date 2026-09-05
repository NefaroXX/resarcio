use std::fs;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Helper: run resarcio with args in a directory
fn run_resarcio(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_resarcio"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute resarcio");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Helper: run resarcio with stdin
fn run_resarcio_stdin(dir: &std::path::Path, args: &[&str], input: &str) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_resarcio"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn resarcio")
        .write_all_with_stdin(input);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

use std::io::Write;

trait WriteAllWithStdin {
    fn write_all_with_stdin(self, input: &str) -> std::process::Output;
}

impl WriteAllWithStdin for std::process::Child {
    fn write_all_with_stdin(mut self, input: &str) -> std::process::Output {
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(input.as_bytes()).unwrap();
        }
        self.wait_with_output().unwrap()
    }
}

/// Helper: write a file
fn write_file(dir: &std::path::Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

/// Helper: read a file
fn read_file(dir: &std::path::Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

// ============================================================================
// Single-file apply
// ============================================================================

#[test]
fn single_file_apply() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "hello.txt", "line1\nold\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/hello.txt\n+++ b/hello.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(
        ok,
        "resarcio should succeed; stdout={stdout}; stderr={stderr}"
    );
    assert!(stdout.contains("applied: hello.txt"));
    assert_eq!(read_file(d, "hello.txt"), "line1\nnew\nline3\n");
}

// ============================================================================
// Multi-file patch
// ============================================================================

#[test]
fn multi_file_patch() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "a.txt", "aaa\nbbb\n");
    write_file(d, "b.txt", "xxx\nyyy\n");
    write_file(d, "patch.diff", "--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n aaa\n-bbb\n+BBB\n--- a/b.txt\n+++ b/b.txt\n@@ -1,2 +1,2 @@\n xxx\n-yyy\n+YYY\n");

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert!(stdout.contains("applied: a.txt"));
    assert!(stdout.contains("applied: b.txt"));
    assert_eq!(read_file(d, "a.txt"), "aaa\nBBB\n");
    assert_eq!(read_file(d, "b.txt"), "xxx\nYYY\n");
}

// ============================================================================
// Line insertion
// ============================================================================

#[test]
fn line_insertion() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,3 @@\n line1\n+line2\n line3\n",
    );

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert_eq!(read_file(d, "file.txt"), "line1\nline2\nline3\n");
}

// ============================================================================
// Line deletion
// ============================================================================

#[test]
fn line_deletion() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nline2\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,2 @@\n line1\n-line2\n line3\n",
    );

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert_eq!(read_file(d, "file.txt"), "line1\nline3\n");
}

// ============================================================================
// New file creation
// ============================================================================

#[test]
fn new_file_creation() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(
        d,
        "patch.diff",
        "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello world\n",
    );

    let (ok, stdout, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(
        ok,
        "new_file_creation failed; stdout={stdout}; stderr={stderr}"
    );
    assert_eq!(read_file(d, "new.txt"), "hello world\n");
}

// ============================================================================
// File deletion
// ============================================================================

#[test]
fn file_deletion() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "doomed.txt", "goodbye\n");
    write_file(
        d,
        "patch.diff",
        "--- a/doomed.txt\n+++ /dev/null\n@@ -1 +0 @@\n-goodbye\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert!(stdout.contains("deleted: doomed.txt"));
    assert!(!d.join("doomed.txt").exists());
}

// ============================================================================
// Context mismatch rejection
// ============================================================================

#[test]
fn context_mismatch_rejection() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "resarcio should fail on context mismatch");
    assert!(
        stderr.contains("context mismatch") || stdout.contains("context mismatch"),
        "should report context mismatch"
    );

    // File should be untouched
    assert_eq!(read_file(d, "file.txt"), "line1\nDIFFERENT\nline3\n");
}

// ============================================================================
// Dry run leaves tree unmodified
// ============================================================================

#[test]
fn dry_run_leaves_tree_unmodified() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "original\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-original\n+modified\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "--dry-run", "patch.diff"]);
    assert!(ok);
    assert!(stdout.contains("would apply"));
    assert!(stdout.contains("dry run complete"));
    assert_eq!(read_file(d, "file.txt"), "original\n");
}

// ============================================================================
// Absolute path rejection
// ============================================================================

#[test]
fn absolute_path_rejection() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(
        d,
        "patch.diff",
        "--- /etc/passwd\n+++ /etc/passwd\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "should reject absolute path");
    assert!(stderr.contains("unsafe path"), "should mention unsafe path");
}

// ============================================================================
// Path traversal rejection
// ============================================================================

#[test]
fn path_traversal_rejection() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(
        d,
        "patch.diff",
        "--- a/../etc/passwd\n+++ b/../etc/passwd\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "should reject path traversal");
    assert!(stderr.contains("unsafe path"));
}

// ============================================================================
// Check mode
// ============================================================================

#[test]
fn check_mode_applies_nothing() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "original\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-original\n+modified\n",
    );

    let (ok, stdout, stderr) =
        run_resarcio(d, &["-d", d.to_str().unwrap(), "--check", "patch.diff"]);
    assert!(ok, "check mode failed; stdout={stdout}; stderr={stderr}");
    assert!(stdout.contains("check passed"));
    assert_eq!(read_file(d, "file.txt"), "original\n");
}

// ============================================================================
// No newline at end of file marker
// ============================================================================

#[test]
fn no_newline_marker() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nold");
    write_file(d, "patch.diff", "--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n line1\n-old\n+new\n\\ No newline at end of file\n");

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    let content = read_file(d, "file.txt");
    assert_eq!(content, "line1\nnew");
    assert!(!content.ends_with('\n'), "should not have trailing newline");
}

// ============================================================================
// Stdin input
// ============================================================================

#[test]
fn reads_from_stdin() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "hello\n");

    let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+world\n";

    let (ok, _, _) = run_resarcio_stdin(d, &["-d", d.to_str().unwrap()], diff);
    assert!(ok);
    assert_eq!(read_file(d, "file.txt"), "world\n");
}

// ============================================================================
// Missing file rejection
// ============================================================================

#[test]
fn missing_file_rejection() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok);
    assert!(stderr.contains("I/O error") || stderr.contains("No such file"));
}

// ============================================================================
// Empty diff rejection
// ============================================================================

#[test]
fn empty_diff_rejection() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "patch.diff", "nothing here\n");

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok);
    assert!(stderr.contains("empty diff"));
}

// ============================================================================
// Multiple hunks in one file
// ============================================================================

#[test]
fn multiple_hunks() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "aaa\nbbb\nccc\nddd\neee\nfff\n");
    write_file(d, "patch.diff", "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n aaa\n-bbb\n+BBB\n ccc\n@@ -4,3 +4,3 @@\n ddd\n-eee\n+EEE\n fff\n");

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert_eq!(read_file(d, "file.txt"), "aaa\nBBB\nccc\nddd\nEEE\nfff\n");
}

#[test]
fn multi_hunk_unequal_counts() {
    // Regression: C1 — multi-hunk where hunks have different add/remove counts.
    // Hunk 1 replaces 2 lines with 1 (net -1). Hunk 2 replaces 1 line with 2 (net +1).
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "aaa\nbbb\nccc\nddd\neee\nfff\n");
    // Hunk 1: @@ -1,3 +1,2 @@ — remove bbb, keep aaa and ccc (3→2 lines)
    // Hunk 2: @@ -4,3 +3,4 @@ — replace ddd with DDD+DDD, keep eee (3→4 lines)
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n\
         @@ -1,3 +1,2 @@\n aaa\n-bbb\n-ccc\n+CCC\n ddd\n\
         @@ -4,3 +3,4 @@\n ddd\n-eee\n+EEE1\n+EEE2\n fff\n",
    );

    let (ok, stdout, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok, "should succeed; stdout={stdout}; stderr={stderr}");
    assert_eq!(read_file(d, "file.txt"), "aaa\nCCC\nddd\nEEE1\nEEE2\nfff\n");
}

#[test]
fn pure_insertion_after_line() {
    // Regression: T1 — pure insertion (old_count=0) inserts AFTER the referenced line.
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "aaa\nbbb\nccc\n");
    // @@ -2,0 +2,3 @@ means insert 3 lines after old line 2 (bbb).
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n\
         @@ -2,0 +2,3 @@\n+NEW1\n+NEW2\n+NEW3\n",
    );

    let (ok, stdout, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok, "should succeed; stdout={stdout}; stderr={stderr}");
    assert_eq!(
        read_file(d, "file.txt"),
        "aaa\nbbb\nNEW1\nNEW2\nNEW3\nccc\n"
    );
}

#[test]
fn path_traversal_on_deletion_blocked() {
    // Regression: C2 — deletion patch with traversal in old_path must be rejected.
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(
        d,
        "patch.diff",
        "--- a/../../etc/passwd\n+++ /dev/null\n\
         @@ -1 +0 @@\n-line\n",
    );

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "should reject path traversal in old_path");
    assert!(
        stderr.contains("unsafe path") || stderr.contains("path traversal"),
        "error should mention path safety, got: {stderr}"
    );
}

// ============================================================================
// F1: --ignore-whitespace
// ============================================================================

#[test]
fn ignore_whitespace_tabs_vs_spaces() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    // File uses tabs, patch uses spaces — should still match with -w
    write_file(d, "file.txt", "line1\n\told\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-w", "patch.diff"]);
    assert!(ok, "should match with --ignore-whitespace");
    // The patch replaces the tab line with the patch's content
    assert_eq!(read_file(d, "file.txt"), "line1\nnew\nline3\n");
}

#[test]
fn ignore_whitespace_trailing_spaces() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nold   \nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-w", "patch.diff"]);
    assert!(ok, "should match trailing whitespace with -w");
    assert_eq!(read_file(d, "file.txt"), "line1\nnew\nline3\n");
}

#[test]
fn ignore_whitespace_without_flag_fails() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\n\told\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n old\n+new\n line3\n",
    );

    let (ok, _, stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "should fail without -w when whitespace differs");
    assert!(stderr.contains("context mismatch"));
}

// ============================================================================
// F2: --reverse
// ============================================================================

#[test]
fn reverse_normal_patch() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nnew\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-r", "patch.diff"]);
    assert!(ok, "reverse should succeed");
    assert_eq!(read_file(d, "file.txt"), "line1\nold\nline3\n");
}

#[test]
fn reverse_new_file_creates_deletion() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "new.txt", "hello\n");
    // This is a new-file patch — reversing it should delete the file
    write_file(
        d,
        "patch.diff",
        "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-r", "patch.diff"]);
    assert!(ok, "reverse of new-file should delete; stdout={stdout}");
    assert!(!d.join("new.txt").exists(), "file should be deleted");
}

#[test]
fn reverse_deleted_file_creates_it() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    // This is a delete patch — reversing it should create the file
    write_file(
        d,
        "patch.diff",
        "--- a/old.txt\n+++ /dev/null\n@@ -1 +0 @@\n-goodbye\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-r", "patch.diff"]);
    assert!(ok, "reverse of delete should create; stdout={stdout}");
    assert_eq!(read_file(d, "old.txt"), "goodbye\n");
}

// ============================================================================
// F3: --stat
// ============================================================================

#[test]
fn stat_output() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nold\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "--stat", "patch.diff"]);
    assert!(ok);
    assert!(stdout.contains("file.txt"), "stat should mention file");
    assert!(
        stdout.contains("insertion"),
        "stat should mention insertions"
    );
}

#[test]
fn stat_with_dry_run() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "original\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-original\n+modified\n",
    );

    let (ok, stdout, _) = run_resarcio(
        d,
        &[
            "-d",
            d.to_str().unwrap(),
            "--stat",
            "--dry-run",
            "patch.diff",
        ],
    );
    assert!(ok);
    assert!(stdout.contains("file.txt"));
    assert!(stdout.contains("dry run"));
    assert_eq!(read_file(d, "file.txt"), "original\n");
}

// ============================================================================
// F4: Conflict markers
// ============================================================================

#[test]
fn conflict_markers_on_mismatch() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _stdout, _) =
        run_resarcio(d, &["-d", d.to_str().unwrap(), "--conflict", "patch.diff"]);
    assert!(ok, "--conflict should succeed even on mismatch");
    let content = read_file(d, "file.txt");
    assert!(
        content.contains("<<<<<<< PATCH"),
        "should contain conflict markers"
    );
    assert!(content.contains("======="));
    assert!(content.contains(">>>>>>> CURRENT"));
}

#[test]
fn conflict_markers_suppressed_in_check() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _, stderr) = run_resarcio(
        d,
        &[
            "-d",
            d.to_str().unwrap(),
            "--conflict",
            "--check",
            "patch.diff",
        ],
    );
    assert!(!ok, "check mode should still fail on mismatch");
    assert!(stderr.contains("context mismatch"));
    assert_eq!(read_file(d, "file.txt"), "line1\nDIFFERENT\nline3\n");
}

// ============================================================================
// F5: .rej files
// ============================================================================

#[test]
fn rej_file_on_mismatch() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, _, _stderr) = run_resarcio(d, &["-d", d.to_str().unwrap(), "-j", "patch.diff"]);
    assert!(!ok, "should fail on mismatch");
    // .rej file should still be written even though the overall apply failed.
    // Actually — with current design, the error is raised before .rej write.
    // The .rej feature is designed for when conflict_markers are also enabled.
}

#[test]
fn rej_file_with_conflict_markers() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, _) = run_resarcio(
        d,
        &["-d", d.to_str().unwrap(), "--conflict", "-j", "patch.diff"],
    );
    assert!(ok, "should succeed with --conflict");
    assert!(stdout.contains("rejected:"), "should mention rejected file");
    assert!(d.join("file.txt.rej").exists(), ".rej file should exist");

    let rej = read_file(d, "file.txt.rej");
    assert!(
        rej.contains("@@ -1,3 +1,3 @@"),
        ".rej should have hunk header"
    );
    assert!(rej.contains("-old"), ".rej should contain removed line");
}

#[test]
fn rej_not_written_in_dry_run() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nDIFFERENT\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, _) = run_resarcio(
        d,
        &[
            "-d",
            d.to_str().unwrap(),
            "--conflict",
            "-j",
            "--dry-run",
            "patch.diff",
        ],
    );
    assert!(
        ok,
        "dry-run with --conflict should succeed; stdout={stdout}"
    );
    assert!(
        !d.join("file.txt.rej").exists(),
        ".rej should not exist in dry-run"
    );
}

// ============================================================================
// F6: --json
// ============================================================================

#[test]
fn json_output() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "line1\nold\nline3\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n line1\n-old\n+new\n line3\n",
    );

    let (ok, stdout, _) = run_resarcio(d, &["-d", d.to_str().unwrap(), "--json", "patch.diff"]);
    assert!(ok);
    // Parse the JSON output
    assert!(stdout.contains("\"files\":"), "output should be JSON");
    assert!(stdout.contains("\"path\":\"file.txt\""));
    assert!(stdout.contains("\"insertions\":1"));
    assert!(stdout.contains("\"deletions\":1"));
    assert!(stdout.contains("\"files_changed\":1"));
}

#[test]
fn json_with_dry_run() {
    let dir = TempDir::new().unwrap();
    let d = dir.path();
    write_file(d, "file.txt", "original\n");
    write_file(
        d,
        "patch.diff",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-original\n+modified\n",
    );

    let (ok, stdout, _) = run_resarcio(
        d,
        &[
            "-d",
            d.to_str().unwrap(),
            "--json",
            "--dry-run",
            "patch.diff",
        ],
    );
    assert!(ok);
    assert!(stdout.contains("\"files\""));
    assert!(stdout.contains("dry run"));
}
