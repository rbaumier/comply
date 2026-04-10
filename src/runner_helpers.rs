//! Shared helpers for the external-tool subprocess runners.
//!
//! Every runner (cargo-shear, cargo-modules, jscpd, knip, madge, …)
//! needs the same three primitives:
//!
//! 1. **Binary probe** — "is this CLI tool installed?"
//! 2. **Workspace root walk** — "given a source file, find the nearest
//!    ancestor directory containing `Cargo.toml` / `package.json`."
//! 3. **Unique-root collection** — "given a list of files, collapse
//!    them down to the set of distinct workspace roots so we don't
//!    invoke the tool twice for the same project."
//!
//! Before this module existed each runner reimplemented all three.
//! Five runners × ~36 lines each was the dominant clone-detection
//! signal in `comply` itself, plus a real Rule of Three violation.
//!
//! The runners still own their availability `OnceLock` because each
//! tool needs its own cached probe — but the BODY of that probe is
//! now `runner_helpers::probe_binary("cargo", &["shear", "--version"])`,
//! which is one line.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::files::SourceFile;

/// True if running `<cmd> <args...>` succeeds. Used by every runner's
/// `is_available()` to test for the presence of the underlying CLI tool.
/// Errors (binary not on PATH, exec failure) all collapse to `false`.
#[must_use]
pub fn probe_binary(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Walk parents of `path` until we find a directory containing
/// `marker_filename` (e.g. `Cargo.toml`, `package.json`). Returns the
/// directory itself — NOT the manifest path. Canonicalizes `path` first
/// so relative paths from the CLI still resolve to absolute roots.
#[must_use]
pub fn find_ancestor_with(path: &Path, marker_filename: &str) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let mut current = canonical.parent();
    while let Some(dir) = current {
        if dir.join(marker_filename).is_file() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

/// Collapse a slice of input files into the set of unique workspace
/// roots they belong to. Files that don't live under any workspace
/// (no ancestor with `marker_filename`) are silently dropped — the
/// caller is expected to either skip them or warn separately.
#[must_use]
pub fn collect_unique_roots(files: &[&SourceFile], marker_filename: &str) -> HashSet<PathBuf> {
    let mut roots = HashSet::new();
    for f in files {
        if let Some(root) = find_ancestor_with(&f.path, marker_filename) {
            roots.insert(root);
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_unknown_binary_returns_false() {
        assert!(!probe_binary("definitely-not-a-real-binary-xyz", &["--version"]));
    }

    #[test]
    fn find_ancestor_returns_none_for_nonexistent_path() {
        assert!(find_ancestor_with(Path::new("/nonexistent/path"), "Cargo.toml").is_none());
    }
}
