//! File discovery — finds lintable files via directory walk or git diff.
//!
//! How it works:
//! 1. ScanMode::All → walks the directory tree respecting .gitignore (via `ignore` crate).
//! 2. Git-based modes → shells out to `git diff --name-only` with appropriate args.
//! 3. Each file is classified by extension into a Language (TS/JS or Rust).
//!    Unknown extensions are silently skipped.

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ScanMode;

const TS_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mts", "mjs"];
const RUST_EXTENSIONS: &[&str] = &["rs"];

/// A discovered file tagged with its detected language.
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    TypeScript,
    Rust,
}

/// Discover files to lint based on the resolved scan mode.
pub fn discover(mode: &ScanMode) -> Result<Vec<SourceFile>> {
    match mode {
        ScanMode::All(path) => walk_directory(path),
        ScanMode::WorkingTree => git_diff_files(&[]),
        ScanMode::Staged => git_diff_files(&["--cached"]),
        ScanMode::LastCommit => git_diff_files(&["HEAD~1"]),
        ScanMode::Commit(sha) => git_diff_files(&[&format!("{sha}~1"), sha]),
        ScanMode::Range(from, to) => git_diff_files(&[from.as_str(), to.as_str()]),
    }
}

/// Walk a directory tree, respecting .gitignore + standard hidden filters,
/// and classify each file.
fn walk_directory(path: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    // standard_filters = .gitignore + .ignore + hidden + parents — without it,
    // we'd walk into .git/, node_modules/, target/, etc. on every invocation.
    let walker = WalkBuilder::new(path).standard_filters(true).build();

    for entry in walker {
        let entry = entry.context("failed to read directory entry")?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        if let Some(sf) = classify(entry.path()) {
            files.push(sf);
        }
    }

    Ok(files)
}

/// Ask git for changed files, then classify each one.
fn git_diff_files(args: &[&str]) -> Result<Vec<SourceFile>> {
    let output = Command::new("git")
        .arg("diff")
        .args(args)
        .args(["--name-only", "--diff-filter=d", "--relative"])
        .output()
        .context("failed to run git diff — is this a git repository?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| classify(Path::new(l)))
        .collect())
}

/// Classify a file path into a Language based on its extension.
/// Returns None for unsupported extensions (silently skipped).
fn classify(path: &Path) -> Option<SourceFile> {
    let ext = path.extension()?.to_str()?;
    let language = if TS_EXTENSIONS.contains(&ext) {
        Language::TypeScript
    } else if RUST_EXTENSIONS.contains(&ext) {
        Language::Rust
    } else {
        return None;
    };
    Some(SourceFile {
        path: path.to_path_buf(),
        language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_recognizes_typescript_extensions() {
        for ext in ["ts", "tsx", "js", "jsx", "mts", "mjs"] {
            let path = PathBuf::from(format!("foo.{ext}"));
            let result = classify(&path);
            assert!(result.is_some(), "{ext} should be recognized");
            assert_eq!(result.unwrap().language, Language::TypeScript);
        }
    }

    #[test]
    fn classify_recognizes_rust_extension() {
        let result = classify(&PathBuf::from("foo.rs"));
        assert!(result.is_some());
        assert_eq!(result.unwrap().language, Language::Rust);
    }

    #[test]
    fn classify_skips_unsupported_extensions() {
        for ext in ["txt", "md", "json", "py"] {
            let path = PathBuf::from(format!("foo.{ext}"));
            assert!(
                classify(&path).is_none(),
                "{ext} must not be classified as a lintable source file"
            );
        }
    }

    #[test]
    fn classify_skips_files_without_extension() {
        assert!(classify(&PathBuf::from("Makefile")).is_none());
    }
}
