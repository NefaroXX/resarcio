use std::fs;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Helper: run accord with args in a directory
fn run_accord(dir: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_accord"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute accord");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stdout, stderr)
}

/// Helper: run accord with stdin
fn run_accord_stdin(dir: &std::path::Path, args: &[&str], input: &str) -> (bool, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_accord"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn accord")
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

    let (ok, stdout, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(
        ok,
        "accord should succeed; stdout={stdout}; stderr={stderr}"
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

    let (ok, stdout, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, stdout, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, stdout, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, stdout, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(!ok, "accord should fail on context mismatch");
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

    let (ok, stdout, _) = run_accord(d, &["-d", d.to_str().unwrap(), "--dry-run", "patch.diff"]);
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

    let (ok, _, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, stdout, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "--check", "patch.diff"]);
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

    let (ok, _, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, _) = run_accord_stdin(d, &["-d", d.to_str().unwrap()], diff);
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

    let (ok, _, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, stderr) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
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

    let (ok, _, _) = run_accord(d, &["-d", d.to_str().unwrap(), "patch.diff"]);
    assert!(ok);
    assert_eq!(read_file(d, "file.txt"), "aaa\nBBB\nccc\nddd\nEEE\nfff\n");
}
