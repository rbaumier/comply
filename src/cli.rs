//! CLI argument parsing.
//!
//! Design: the default invocation `comply [path]` lints files and is the
//! hottest code path. Optional subcommands (`explain`, `list`) provide
//! introspection tooling without disrupting the lint flow. When no
//! subcommand is passed, we fall into `Command::Lint` with the legacy flags.

use clap::{ArgGroup, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "comply", about = "Your code will comply.", version)]
#[command(group(
    ArgGroup::new("scan_mode")
        .args(["working_tree", "staged", "last_commit", "commit", "range"])
        .multiple(false)
))]
pub struct Cli {
    /// Optional subcommand. Default = lint.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Lint files modified in the working tree.
    #[arg(long, global = false)]
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

    /// Output diagnostics as JSON (for editors and CI).
    #[arg(long)]
    pub json: bool,

    /// Path to lint (default: current directory).
    pub path: Option<PathBuf>,
}

/// Top-level subcommands. None = lint mode with the legacy flag parser.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Show the full description and remediation for a specific rule.
    Explain {
        /// The stable rule id, e.g. "no-throw" or "typescript/no-explicit-any".
        rule_id: String,
    },
    /// List every registered rule with its id, severity, and description.
    List {
        /// Output as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
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
            command: None,
            working_tree: false,
            staged: false,
            last_commit: false,
            commit: None,
            range: None,
            json: false,
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
