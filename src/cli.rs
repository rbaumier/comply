//! CLI argument parsing — produces a single ScanMode that the orchestrator
//! consumes. The mutex flags (`--working-tree`, `--staged`, etc.) are wired
//! into a clap ArgGroup so the user gets a clear error if they combine them.

use clap::{ArgGroup, Parser};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "comply", about = "Your code will comply.", version)]
#[command(group(
    ArgGroup::new("scan_mode")
        .args(["working_tree", "staged", "last_commit", "commit", "range"])
        .multiple(false)
))]
pub struct Cli {
    /// Lint files modified in the working tree.
    #[arg(long)]
    pub working_tree: bool,

    /// Lint staged files only.
    #[arg(long)]
    pub staged: bool,

    /// Lint files changed in the last commit.
    #[arg(long)]
    pub last_commit: bool,

    /// Lint files changed in a specific commit.
    #[arg(long)]
    pub commit: Option<String>,

    /// Lint files changed between two commits (FROM TO).
    #[arg(long, num_args = 2)]
    pub range: Option<Vec<String>>,

    /// Path to lint (default: current directory).
    pub path: Option<PathBuf>,
}

/// Resolved scan mode — determines which files comply will lint.
pub enum ScanMode {
    All(PathBuf),
    WorkingTree,
    Staged,
    LastCommit,
    Commit(String),
    Range(String, String),
}

impl Cli {
    pub fn scan_mode(&self) -> ScanMode {
        if self.working_tree {
            return ScanMode::WorkingTree;
        }
        if self.staged {
            return ScanMode::Staged;
        }
        if self.last_commit {
            return ScanMode::LastCommit;
        }
        if let Some(sha) = &self.commit {
            return ScanMode::Commit(sha.clone());
        }
        if let Some(range) = &self.range {
            // clap's `num_args = 2` enforces exactly two values, so direct
            // indexing is safe — but we destructure to make that explicit.
            let [from, to] = [range[0].clone(), range[1].clone()];
            return ScanMode::Range(from, to);
        }
        ScanMode::All(self.path.clone().unwrap_or_else(|| PathBuf::from(".")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_with_defaults() -> Cli {
        Cli {
            working_tree: false,
            staged: false,
            last_commit: false,
            commit: None,
            range: None,
            path: None,
        }
    }

    #[test]
    fn default_returns_all_mode_with_current_dir() {
        let cli = cli_with_defaults();
        match cli.scan_mode() {
            ScanMode::All(p) => assert_eq!(p, PathBuf::from(".")),
            _ => panic!("expected ScanMode::All"),
        }
    }

    #[test]
    fn explicit_path_returns_all_mode_with_that_path() {
        let mut cli = cli_with_defaults();
        cli.path = Some(PathBuf::from("src/"));
        match cli.scan_mode() {
            ScanMode::All(p) => assert_eq!(p, PathBuf::from("src/")),
            _ => panic!("expected ScanMode::All"),
        }
    }

    #[test]
    fn working_tree_flag_returns_working_tree_mode() {
        let mut cli = cli_with_defaults();
        cli.working_tree = true;
        assert!(matches!(cli.scan_mode(), ScanMode::WorkingTree));
    }

    #[test]
    fn commit_flag_carries_sha() {
        let mut cli = cli_with_defaults();
        cli.commit = Some("abc123".into());
        match cli.scan_mode() {
            ScanMode::Commit(sha) => assert_eq!(sha, "abc123"),
            _ => panic!("expected ScanMode::Commit"),
        }
    }

    #[test]
    fn range_flag_carries_both_refs() {
        let mut cli = cli_with_defaults();
        cli.range = Some(vec!["v1".into(), "v2".into()]);
        match cli.scan_mode() {
            ScanMode::Range(from, to) => {
                assert_eq!(from, "v1");
                assert_eq!(to, "v2");
            }
            _ => panic!("expected ScanMode::Range"),
        }
    }
}
