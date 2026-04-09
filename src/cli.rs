use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "comply", about = "Your code will comply.", version)]
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
            ScanMode::WorkingTree
        } else if self.staged {
            ScanMode::Staged
        } else if self.last_commit {
            ScanMode::LastCommit
        } else if let Some(sha) = &self.commit {
            ScanMode::Commit(sha.clone())
        } else if let Some(range) = &self.range {
            ScanMode::Range(range[0].clone(), range[1].clone())
        } else {
            ScanMode::All(self.path.clone().unwrap_or_else(|| PathBuf::from(".")))
        }
    }
}
