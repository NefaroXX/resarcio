# accord

A CLI tool that applies unified diffs to files — the write-side counterpart to `diff`.

Part of the [NefaroXX](https://github.com/NefaroXX) ecosystem:

| Tool | Role |
|------|------|
| [**ruling**](https://github.com/NefaroXX/ruling) | Read side — computes diffs between files |
| [**accord**](https://github.com/NefaroXX/accord) | Write side — applies unified patches to a working tree |

## Features

- Apply unified diffs (git-style `---`/`+++`/`@@`) to files in a target directory
- Multi-file patches in a single diff
- New file creation (`--- /dev/null`)
- File deletion (`+++ /dev/null`)
- Dry-run mode (`-n` / `--dry-run`)
- Check mode (`-c` / `--check`) — verify without writing
- Stdin input (omit the patch file argument)
- `\ No newline at end of file` support
- Path traversal and symlink escape protection

## Install

```sh
cargo install accord
```

## Usage

```sh
# Apply a patch file
accord -d /path/to/project fix.diff

# Dry run — show what would change
accord -d /path/to/project -n fix.diff

# Check mode — verify patch applies cleanly
accord -d /path/to/project -c fix.diff

# Read from stdin
cat fix.diff | accord -d /path/to/project
```

## Diff format

Standard unified diff:

```diff
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line1
-old
+new
 line3
```

## License

MIT
