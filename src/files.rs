//! File discovery — finds lintable files via directory walk or git diff.
//!
//! How it works:
//! 1. ScanMode::All → walks the directory tree using `ignore` crate's
//!    `standard_filters` (.gitignore + .ignore + hidden + parent traversal).
//!    Without standard_filters we'd descend into .git/, node_modules/, target/.
//! 2. Git-based modes → shells out to `git diff --name-only` (or git show
//!    for single-commit mode) and validates the exit status. Silent empty
//!    output on failure used to mask real errors.
//! 3. Each file is classified by extension into a Language. Unknown
//!    extensions are silently skipped — letting users point comply at a
//!    mixed-language repo without noise.

use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ScanMode;

const TS_EXTENSIONS: &[&str] = &["ts", "mts"];
const TSX_EXTENSIONS: &[&str] = &["tsx", "jsx"];
const JS_EXTENSIONS: &[&str] = &["js", "mjs"];
const RUST_EXTENSIONS: &[&str] = &["rs"];

/// A discovered file tagged with its detected language.
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// The detected source language. TS and Tsx are kept distinct so the engine
/// can pick the correct tree-sitter grammar — TSX requires `LANGUAGE_TSX`,
/// otherwise JSX syntax produces ERROR nodes and bogus diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Plain `.ts` / `.mts`.
    TypeScript,
    /// `.tsx` / `.jsx` — needs the JSX-aware grammar.
    Tsx,
    /// Plain JavaScript `.js` / `.mjs` — handled by the TypeScript grammar
    /// since it's a strict superset.
    JavaScript,
    /// Rust source `.rs` — no tree-sitter grammar bundled in v1.
    Rust,
}

impl Language {
    /// True if the language is a TypeScript/JavaScript variant — used by the
    /// orchestrator to dispatch to oxlint.
    pub fn is_typescript_family(self) -> bool {
        matches!(
            self,
            Language::TypeScript | Language::Tsx | Language::JavaScript
        )
    }
}

/// Discover files to lint based on the resolved scan mode.
#[must_use = "discovered files must be linted or the scan was wasted"]
pub fn discover(mode: &ScanMode) -> Result<Vec<SourceFile>> {
    match mode {
        ScanMode::All(path) => walk_directory(path),
        ScanMode::WorkingTree => git_diff_files(&[]),
        ScanMode::Staged => git_diff_files(&["--cached"]),
        // `git diff HEAD~1 HEAD` — without the second `HEAD`, git diffs against
        // the working tree, mixing unstaged changes into "last commit" results.
        ScanMode::LastCommit => git_diff_files(&["HEAD~1", "HEAD"]),
        ScanMode::Commit(sha) => git_show_files(sha),
        ScanMode::Range(from, to) => git_diff_files(&[from.as_str(), to.as_str()]),
    }
}

/// Walk a directory tree and classify every file.
fn walk_directory(path: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
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

/// Run `git diff --name-only` with the given args, validate exit status,
/// then classify each output line.
fn git_diff_files(args: &[&str]) -> Result<Vec<SourceFile>> {
    let output = Command::new("git")
        .arg("diff")
        .args(args)
        .args(["--name-only", "--diff-filter=d", "--relative"])
        .output()
        .context("failed to invoke git — is git installed and PATH correct?")?;

    if !output.status.success() {
        bail!(
            "git diff failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    parse_git_output(&output.stdout)
}

/// Run `git show --name-only` for a single commit. Handles initial commits
/// and merge commits correctly (which `git diff <sha>~1 <sha>` does not).
fn git_show_files(sha: &str) -> Result<Vec<SourceFile>> {
    let output = Command::new("git")
        .args(["show", "--name-only", "--pretty=format:", "--diff-filter=d"])
        .arg(sha)
        .output()
        .context("failed to invoke git — is git installed and PATH correct?")?;

    if !output.status.success() {
        bail!(
            "git show failed for {sha} (exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    parse_git_output(&output.stdout)
}

/// Parse git output line-by-line. Strict UTF-8 — non-UTF-8 paths bail loudly
/// rather than being silently corrupted by `from_utf8_lossy`.
fn parse_git_output(stdout: &[u8]) -> Result<Vec<SourceFile>> {
    let text = std::str::from_utf8(stdout)
        .context("git output contained non-UTF-8 bytes — paths cannot be safely processed")?;
    Ok(text
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
    } else if TSX_EXTENSIONS.contains(&ext) {
        Language::Tsx
    } else if JS_EXTENSIONS.contains(&ext) {
        Language::JavaScript
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
    fn classify_recognizes_plain_typescript() {
        for ext in ["ts", "mts"] {
            let r = classify(&PathBuf::from(format!("foo.{ext}"))).unwrap();
            assert_eq!(r.language, Language::TypeScript);
        }
    }

    #[test]
    fn classify_recognizes_tsx_and_jsx_as_tsx_variant() {
        for ext in ["tsx", "jsx"] {
            let r = classify(&PathBuf::from(format!("foo.{ext}"))).unwrap();
            assert_eq!(r.language, Language::Tsx, "{ext} must use the TSX grammar");
        }
    }

    #[test]
    fn classify_recognizes_javascript() {
        for ext in ["js", "mjs"] {
            let r = classify(&PathBuf::from(format!("foo.{ext}"))).unwrap();
            assert_eq!(r.language, Language::JavaScript);
        }
    }

    #[test]
    fn classify_recognizes_rust() {
        let r = classify(&PathBuf::from("foo.rs")).unwrap();
        assert_eq!(r.language, Language::Rust);
    }

    #[test]
    fn classify_skips_unsupported_extensions() {
        for ext in ["txt", "md", "json", "py"] {
            assert!(classify(&PathBuf::from(format!("foo.{ext}"))).is_none());
        }
    }

    #[test]
    fn classify_skips_files_without_extension() {
        assert!(classify(&PathBuf::from("Makefile")).is_none());
    }

    #[test]
    fn is_typescript_family_groups_correctly() {
        assert!(Language::TypeScript.is_typescript_family());
        assert!(Language::Tsx.is_typescript_family());
        assert!(Language::JavaScript.is_typescript_family());
        assert!(!Language::Rust.is_typescript_family());
    }

    #[test]
    fn parse_git_output_handles_strict_utf8() {
        let result = parse_git_output(b"src/foo.ts\nsrc/bar.rs\n").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_git_output_rejects_invalid_utf8() {
        // Invalid UTF-8 byte sequence — must error, not corrupt silently.
        assert!(parse_git_output(&[0xFF, 0xFE, b'\n']).is_err());
    }
}
