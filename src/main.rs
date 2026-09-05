mod apply;
mod error;
mod parse;
mod safety;

use clap::Parser as ClapParser;
use std::io::Read;
use std::path::PathBuf;
use std::process;

/// resarcio - Apply unified diffs to a working tree
///
/// The write-side counterpart to diff. Applies unified diff patches
/// (git-style) to files in a target directory.
#[derive(ClapParser)]
#[command(name = "resarcio", version, about)]
struct Cli {
    /// Target directory to apply the patch to (default: current directory)
    #[arg(short = 'd', long = "directory", default_value = ".")]
    directory: PathBuf,

    /// Dry run — show what would be changed without writing files
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Check mode — verify the patch applies cleanly without modifying files
    #[arg(short = 'c', long = "check")]
    check: bool,

    /// Apply the patch in reverse (undo the change)
    #[arg(short = 'r', long = "reverse")]
    reverse: bool,

    /// Ignore whitespace differences when matching context lines
    #[arg(short = 'w', long = "ignore-whitespace")]
    ignore_whitespace: bool,

    /// Insert conflict markers on hunk mismatch instead of failing
    #[arg(long = "conflict")]
    conflict: bool,

    /// Write failed hunks to .rej sidecar files
    #[arg(short = 'j', long = "reject")]
    reject: bool,

    /// Show per-file change summary (insertions/deletions)
    #[arg(long = "stat")]
    stat: bool,

    /// Output results as JSON (machine-readable)
    #[arg(long = "json")]
    json: bool,

    /// Unified diff file to apply (reads from stdin if omitted)
    patch_file: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match run(cli) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("resarcio: error: {}", e);
            1
        }
    };

    process::exit(exit_code);
}

fn run(cli: Cli) -> Result<(), error::ResarcioError> {
    // Read diff input from file or stdin
    let diff_input = match &cli.patch_file {
        Some(path) => {
            let mut file = std::fs::File::open(path).map_err(error::ResarcioError::Io)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(error::ResarcioError::Io)?;
            contents
        }
        None => {
            let mut contents = String::new();
            std::io::stdin()
                .read_to_string(&mut contents)
                .map_err(error::ResarcioError::Io)?;
            contents
        }
    };

    // Parse the diff
    let mut diff = parse::parse_diff(&diff_input)?;

    if diff.files.is_empty() {
        return Err(error::ResarcioError::EmptyDiff);
    }

    // Reverse if requested
    if cli.reverse {
        diff = parse::reverse_diff(&diff);
    }

    // Validate target directory
    let target_dir = if cli.directory.exists() {
        if !cli.directory.is_dir() {
            return Err(error::ResarcioError::IsADirectory(
                cli.directory.display().to_string(),
            ));
        }
        cli.directory.clone()
    } else {
        return Err(error::ResarcioError::FileNotFound(
            cli.directory.display().to_string(),
        ));
    };

    // Validate all paths before applying anything
    for file_patch in &diff.files {
        if file_patch.new_path != "/dev/null" && file_patch.new_path != "dev/null" {
            let validated = safety::validate_path(&file_patch.new_path)?;
            safety::resolve_within(&target_dir, &validated)?;
        }
        if file_patch.old_path != "/dev/null" && file_patch.old_path != "dev/null" {
            let validated = safety::validate_path(&file_patch.old_path)?;
            safety::resolve_within(&target_dir, &validated)?;
        }
    }

    // Check mode and dry-run are read-only
    let dry_run = cli.dry_run || cli.check;

    let options = apply::ApplyOptions {
        dry_run,
        check: cli.check,
        reverse: cli.reverse,
        ignore_whitespace: cli.ignore_whitespace,
        conflict_markers: cli.conflict,
        write_rej: cli.reject && !dry_run,
    };

    // Apply each file patch and collect results
    let mut report = apply::ApplyReport { files: Vec::new() };

    for file_patch in &diff.files {
        let result = apply::apply_file_patch(&target_dir, file_patch, &options)?;
        report.files.push(result);
    }

    // Output
    if cli.json {
        println!("{}", apply::format_json(&report));
    } else if cli.stat {
        print!("{}", apply::format_stat(&report));
    }

    if cli.dry_run {
        println!("resarcio: dry run complete — no files were modified");
    } else if cli.check {
        println!("resarcio: check passed — patch applies cleanly");
    }

    Ok(())
}
