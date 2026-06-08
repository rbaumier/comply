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

use rustc_hash::FxHashSet;
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

/// Walk parents of `path` to the root of the *cargo workspace* it
/// belongs to: the nearest ancestor `Cargo.toml` that declares a
/// `[workspace]` table. Falls back to the nearest ancestor `Cargo.toml`
/// (a standalone crate) when none declares a workspace. Returns the
/// directory, not the manifest path.
///
/// This is the cargo-aware counterpart to [`find_ancestor_with`], which
/// stops at the *first* `Cargo.toml`. For a workspace member that first
/// manifest is the member's own — grouping by it makes a runner compile
/// each member separately instead of once per workspace. Resolving to the
/// workspace root lets one `cargo` invocation cover every member with a
/// shared build.
///
/// Detection is a textual `[workspace]` probe, so it does not honor
/// `workspace.exclude`: a file under an excluded member is still
/// attributed to the parent workspace.
///
/// Pick this resolver for runners that accept `--workspace` and benefit
/// from a shared build (clippy lint, cargo-shear). Pick [`find_ancestor_with`]
/// for tools that must stay per-package — either because the tool has no
/// workspace mode (`cargo modules orphans` walks one crate's module tree)
/// or because a narrow blast radius is wanted (`clippy --fix` should only
/// touch the member crates the user actually edited, not their siblings).
#[must_use]
pub fn find_cargo_workspace_root(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let mut current = canonical.parent();
    let mut nearest = None;
    while let Some(dir) = current {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.is_file() {
            if nearest.is_none() {
                nearest = Some(dir.to_path_buf());
            }
            if std::fs::read_to_string(&cargo_toml)
                .is_ok_and(|content| content.contains("[workspace]"))
            {
                return Some(dir.to_path_buf());
            }
        }
        current = dir.parent();
    }
    nearest
}

/// Collapse a slice of input files into the set of unique workspace
/// roots they belong to. Files that don't live under any workspace
/// (no ancestor with `marker_filename`) are silently dropped — the
/// caller is expected to either skip them or warn separately.
#[must_use]
pub fn collect_unique_roots(files: &[&SourceFile], marker_filename: &str) -> FxHashSet<PathBuf> {
    let mut roots = FxHashSet::default();
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
        assert!(!probe_binary(
            "definitely-not-a-real-binary-xyz",
            &["--version"]
        ));
    }

    #[test]
    fn find_ancestor_returns_none_for_nonexistent_path() {
        assert!(find_ancestor_with(Path::new("/nonexistent/path"), "Cargo.toml").is_none());
    }

    #[test]
    fn workspace_root_resolves_member_to_root_with_workspace_table() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"member\"]\n",
        )
        .unwrap();
        let member_src = root.join("member").join("src");
        std::fs::create_dir_all(&member_src).unwrap();
        std::fs::write(
            root.join("member").join("Cargo.toml"),
            "[package]\nname = \"member\"\n",
        )
        .unwrap();
        let file = member_src.join("lib.rs");
        std::fs::write(&file, "").unwrap();

        // Resolves to the workspace root, NOT the member manifest, so one
        // clippy run covers every member.
        assert_eq!(
            find_cargo_workspace_root(&file),
            Some(std::fs::canonicalize(root).unwrap())
        );
    }

    #[test]
    fn workspace_root_is_none_when_no_ancestor_manifest() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("loose.rs");
        std::fs::write(&file, "").unwrap();
        // No Cargo.toml in this branch (TempDir lives under the system temp
        // dir, which has no manifest) → neither a workspace nor a crate.
        assert_eq!(find_cargo_workspace_root(&file), None);
    }

    #[test]
    fn workspace_root_is_noop_for_standalone_crate() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let file = src.join("main.rs");
        std::fs::write(&file, "").unwrap();

        // No `[workspace]` anywhere: falls back to the crate's own dir,
        // matching the previous nearest-Cargo.toml behavior.
        assert_eq!(
            find_cargo_workspace_root(&file),
            Some(std::fs::canonicalize(root).unwrap())
        );
    }
}
