//! File discovery — finds lintable files via directory walk or git diff.
//!
//! - `ScanMode::All` → directory walk via `ignore` crate (standard_filters
//!   excludes .git/, node_modules/, target/).
//! - Git modes → shell out to `git diff` / `git show` and validate exit
//!   status (silent empty output used to mask real failures).
//! - Each file is classified by extension into a Language; unknown
//!   extensions are silently skipped.

use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ScanMode;

const TS_EXTENSIONS: &[&str] = &["ts", "mts"];
const TSX_EXTENSIONS: &[&str] = &["tsx", "jsx"];
const JS_EXTENSIONS: &[&str] = &["js", "mjs"];
const RUST_EXTENSIONS: &[&str] = &["rs"];
const VUE_EXTENSIONS: &[&str] = &["vue"];
const TOML_EXTENSIONS: &[&str] = &["toml"];
const JSON_EXTENSIONS: &[&str] = &["json"];
const CSS_EXTENSIONS: &[&str] = &["css"];
const YAML_EXTENSIONS: &[&str] = &["yml", "yaml"];
const DOCKERFILE_EXTENSIONS: &[&str] = &["dockerfile"];
const SQL_EXTENSIONS: &[&str] = &["sql"];
const GRAPHQL_EXTENSIONS: &[&str] = &["graphql", "gql"];

/// A discovered file tagged with its detected language.
#[derive(Debug)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// The detected source language. TS and Tsx are kept distinct so the engine
/// can pick the correct tree-sitter grammar — TSX requires `LANGUAGE_TSX`,
/// otherwise JSX syntax produces ERROR nodes and bogus diagnostics.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Plain `.ts` / `.mts`.
    TypeScript,
    /// `.tsx` / `.jsx` — needs the JSX-aware grammar.
    Tsx,
    /// Plain JavaScript `.js` / `.mjs` — handled by the TypeScript grammar
    /// since it's a strict superset.
    JavaScript,
    /// Rust source `.rs`.
    Rust,
    /// Vue Single-File Component `.vue` — text-based rules only (no
    /// tree-sitter grammar bundled). Rules check the raw SFC source
    /// for template/script patterns.
    Vue,
    /// TOML configuration file `.toml` — text-based rules only, parsed
    /// on demand by individual rules via the `toml` crate.
    Toml,
    /// JSON data file `.json` — text-based rules only, parsed on demand
    /// by individual rules via the `serde_json` crate. Used for i18n
    /// translation files, config files, etc.
    Json,
    /// CSS stylesheet `.css` — text-based rules only.
    Css,
    /// YAML file `.yml` / `.yaml` — text-based rules only. Used for
    /// Kubernetes manifests, docker-compose, GitHub Actions workflows.
    Yaml,
    /// Dockerfile — text-based rules only. Matched by extension
    /// `.dockerfile` or by filename starting with `Dockerfile`.
    Dockerfile,
    /// SQL file `.sql` — text-based rules only.
    Sql,
    /// GraphQL schema or operation file `.graphql` / `.gql` — text-based
    /// rules only.
    GraphQl,
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

    /// Detect the language from a file path's extension. Returns `None`
    /// for extensions comply doesn't recognize. Used by the LSP server,
    /// which receives URIs from the editor and needs to decide whether
    /// the buffer is in scope before running the lint pass.
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        if name.ends_with(".d.ts") || name.ends_with(".d.mts") {
            return None;
        }
        let ext = path.extension()?.to_str()?;
        if TS_EXTENSIONS.contains(&ext) {
            Some(Language::TypeScript)
        } else if TSX_EXTENSIONS.contains(&ext) {
            Some(Language::Tsx)
        } else if JS_EXTENSIONS.contains(&ext) {
            Some(Language::JavaScript)
        } else if RUST_EXTENSIONS.contains(&ext) {
            Some(Language::Rust)
        } else if VUE_EXTENSIONS.contains(&ext) {
            Some(Language::Vue)
        } else if TOML_EXTENSIONS.contains(&ext) {
            Some(Language::Toml)
        } else if JSON_EXTENSIONS.contains(&ext) {
            Some(Language::Json)
        } else if CSS_EXTENSIONS.contains(&ext) {
            Some(Language::Css)
        } else if YAML_EXTENSIONS.contains(&ext) {
            Some(Language::Yaml)
        } else if SQL_EXTENSIONS.contains(&ext) {
            Some(Language::Sql)
        } else if GRAPHQL_EXTENSIONS.contains(&ext) {
            Some(Language::GraphQl)
        } else if DOCKERFILE_EXTENSIONS.contains(&ext)
            || path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n == "Dockerfile" || n.starts_with("Dockerfile.")
            })
        {
            Some(Language::Dockerfile)
        } else {
            None
        }
    }
}

/// Discover files to lint based on the resolved scan mode.
#[must_use = "discovered files must be linted or the scan was wasted"]
pub fn discover(mode: &ScanMode) -> Result<Vec<SourceFile>> {
    match mode {
        ScanMode::All(path) => walk_directory(path),
        ScanMode::WorkingTree => git_diff_files(&[]),
        ScanMode::Staged => git_diff_files(&["--cached"]),
        // `HEAD~1 HEAD` — without the second `HEAD`, git diffs against the
        // working tree and mixes unstaged changes into "last commit" results.
        ScanMode::LastCommit => git_diff_files(&["HEAD~1", "HEAD"]),
        ScanMode::Commit(sha) => git_show_files(sha),
        ScanMode::Range(from, to) => git_diff_files(&[from.as_str(), to.as_str()]),
    }
}

const ALWAYS_SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", ".git"];

/// Walk a directory tree and classify every file.
fn walk_directory(path: &Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(path)
        .standard_filters(true)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                if let Some(name) = entry.file_name().to_str() {
                    return !ALWAYS_SKIP_DIRS.contains(&name);
                }
            }
            true
        })
        .build();
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

/// `git diff --name-only` with the given args. Used for working-tree, staged,
/// last-commit, and range modes.
fn git_diff_files(args: &[&str]) -> Result<Vec<SourceFile>> {
    let mut cmd = Command::new("git");
    cmd.arg("diff")
        .args(args)
        .args(["--name-only", "--diff-filter=d", "--relative"]);
    capture_git_output(cmd, "git diff")
}

/// `git show --name-only` for a single commit — handles initial and merge
/// commits, which `git diff <sha>~1 <sha>` cannot.
fn git_show_files(sha: &str) -> Result<Vec<SourceFile>> {
    let mut cmd = Command::new("git");
    cmd.args(["show", "--name-only", "--pretty=format:", "--diff-filter=d"])
        .arg(sha);
    capture_git_output(cmd, "git show")
}

/// Spawn git, validate exit status, then classify the output paths.
/// Centralizes the bail-on-error pattern so future git modes can't forget it.
fn capture_git_output(mut cmd: Command, label: &str) -> Result<Vec<SourceFile>> {
    let output = cmd
        .output()
        .context("failed to invoke git — is git installed and on PATH?")?;
    if !output.status.success() {
        bail!(
            "{label} failed (exit {}): {}",
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
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.ends_with(".d.ts") || name.ends_with(".d.mts") {
            return None;
        }
    }
    let language = if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if TS_EXTENSIONS.contains(&ext) {
            Language::TypeScript
        } else if TSX_EXTENSIONS.contains(&ext) {
            Language::Tsx
        } else if JS_EXTENSIONS.contains(&ext) {
            Language::JavaScript
        } else if RUST_EXTENSIONS.contains(&ext) {
            Language::Rust
        } else if VUE_EXTENSIONS.contains(&ext) {
            Language::Vue
        } else if TOML_EXTENSIONS.contains(&ext) {
            Language::Toml
        } else if JSON_EXTENSIONS.contains(&ext) {
            Language::Json
        } else if CSS_EXTENSIONS.contains(&ext) {
            Language::Css
        } else if YAML_EXTENSIONS.contains(&ext) {
            Language::Yaml
        } else if SQL_EXTENSIONS.contains(&ext) {
            Language::Sql
        } else if GRAPHQL_EXTENSIONS.contains(&ext) {
            Language::GraphQl
        } else if DOCKERFILE_EXTENSIONS.contains(&ext) {
            Language::Dockerfile
        } else {
            return None;
        }
    } else if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n == "Dockerfile" || n.starts_with("Dockerfile.")
    }) {
        Language::Dockerfile
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

    fn lang_for(ext: &str) -> Language {
        classify(&PathBuf::from(format!("foo.{ext}"))).unwrap().language
    }

    #[test]
    fn classify_routes_extension_to_language() {
        for ext in ["ts", "mts"] {
            assert_eq!(lang_for(ext), Language::TypeScript);
        }
        for ext in ["tsx", "jsx"] {
            assert_eq!(lang_for(ext), Language::Tsx, "{ext} → TSX grammar");
        }
        for ext in ["js", "mjs"] {
            assert_eq!(lang_for(ext), Language::JavaScript);
        }
        assert_eq!(lang_for("rs"), Language::Rust);
    }

    #[test]
    fn classify_skips_unsupported_or_extensionless() {
        for ext in ["txt", "md", "py"] {
            assert!(classify(&PathBuf::from(format!("foo.{ext}"))).is_none());
        }
        assert!(classify(&PathBuf::from("Makefile")).is_none());
    }

    #[test]
    fn classify_json_files() {
        assert_eq!(lang_for("json"), Language::Json);
        assert_eq!(lang_for("toml"), Language::Toml);
    }

    #[test]
    fn is_typescript_family_groups_correctly() {
        assert!(Language::TypeScript.is_typescript_family());
        assert!(Language::Tsx.is_typescript_family());
        assert!(Language::JavaScript.is_typescript_family());
        assert!(!Language::Rust.is_typescript_family());
    }

    #[test]
    fn parse_git_output_strict_utf8() {
        assert_eq!(parse_git_output(b"a.ts\nb.rs\n").unwrap().len(), 2);
        // Invalid UTF-8 byte sequence — must error, not corrupt silently.
        assert!(parse_git_output(&[0xFF, 0xFE, b'\n']).is_err());
    }
}
