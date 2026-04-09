//! Auto-fix orchestration: run the fixers built into oxlint and clippy.
//!
//! Comply doesn't ship its own fixers — most of the in-process
//! tree-sitter rules are architectural (max-function-lines, naming,
//! Law of Demeter…) and can't be safely auto-fixed. The two upstream
//! tools comply already delegates to (`oxlint` and `cargo clippy`)
//! both have battle-tested fixers for the rules they own, so the
//! best comply can do is forward `--fix` to them and let them edit
//! source files in place.
//!
//! Pipeline:
//!   1. `comply --fix <path>` discovers files (same scan logic as
//!      a normal lint run).
//!   2. For TS/JS files: invoke `oxlint --fix -- <files>`. The
//!      generated oxlintrc still controls which rules are eligible.
//!   3. For Rust files: group by workspace (same as the normal clippy
//!      runner), then invoke `cargo clippy --fix --allow-dirty
//!      --allow-staged --manifest-path X -- <lint args>`. The
//!      `--allow-dirty/--allow-staged` flags let clippy edit a tree
//!      with uncommitted changes; without them, clippy refuses.
//!   4. After both tools finish, comply re-runs the normal lint pass
//!      so the user sees what's left (the diagnostics nobody can
//!      auto-fix). The caller of `apply_fixes` is responsible for
//!      that re-run; this module just edits the files.
//!
//! Loose `.rs` files outside any Cargo workspace are skipped with a
//! warning, same as the regular clippy runner.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::files::{Language, SourceFile};

/// Run every available auto-fixer on the discovered files. Returns
/// the number of fixer invocations that exited successfully — useful
/// for the caller's "comply: ran N fixers" status line.
pub fn apply_fixes(discovered: &[SourceFile], config: &Config) -> Result<usize> {
    let mut runs = 0;
    let ts_files: Vec<&SourceFile> = discovered
        .iter()
        .filter(|f| f.language.is_typescript_family())
        .collect();
    let rs_files: Vec<&SourceFile> = discovered
        .iter()
        .filter(|f| f.language == Language::Rust)
        .collect();

    if !ts_files.is_empty() && oxlint_available() {
        match run_oxlint_fix(&ts_files) {
            Ok(()) => runs += 1,
            Err(e) => eprintln!("comply: oxlint --fix failed: {e:#}"),
        }
    }

    if !rs_files.is_empty() && clippy_available() {
        runs += run_clippy_fix(&rs_files, config)?;
    }

    Ok(runs)
}

/// Cheap availability probe — same idea as `oxlint::is_available`,
/// duplicated here so the fix module doesn't reach into the lint
/// module's private cache. Both call sites end up running once per
/// invocation, which is negligible.
fn oxlint_available() -> bool {
    Command::new("oxlint")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn clippy_available() -> bool {
    Command::new("cargo")
        .args(["clippy", "--version"])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Spawn `oxlint --fix --config <generated> -- <files>` on the TS/JS
/// batch. We pass the same generated oxlintrc the lint mode uses, so
/// `--fix` only edits rules comply considers active. Without the
/// config flag, oxlint would run with defaults and miss most fixes.
fn run_oxlint_fix(files: &[&SourceFile]) -> Result<()> {
    let bindings = crate::rules::collect_oxlint_bindings();
    let rule_entries: Vec<(&str, crate::diagnostic::Severity)> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev))
        .collect();
    let config_file = crate::oxlint_config::generate(&rule_entries)
        .context("failed to generate oxlintrc for --fix run")?;

    let mut cmd = Command::new("oxlint");
    cmd.arg("--fix");
    cmd.arg("-c").arg(config_file.path());
    cmd.arg("--");
    for f in files {
        cmd.arg(&f.path);
    }
    let status = cmd
        .status()
        .context("failed to invoke `oxlint --fix` — is it installed?")?;
    if !status.success() && status.code() != Some(1) {
        // oxlint exits 1 when there are still violations after fixing,
        // which is the normal case — don't treat it as an error.
        anyhow::bail!("oxlint --fix exited with status {status}");
    }
    // `config_file` keeps the temp file alive until here so oxlint
    // could read it; dropping it now removes the file from disk.
    drop(config_file);
    Ok(())
}

/// Run `cargo clippy --fix --allow-dirty --allow-staged` once per
/// distinct workspace touched by the input files. Returns the
/// successful invocation count.
fn run_clippy_fix(files: &[&SourceFile], config: &Config) -> Result<usize> {
    let mut runs = 0;
    let workspaces = group_by_workspace(files);
    let mut skipped: Vec<String> = Vec::new();

    for (workspace, files_in_ws) in workspaces {
        match workspace {
            Some(root) => {
                let touched: HashSet<&Path> =
                    files_in_ws.iter().map(|f| f.path.as_path()).collect();
                if let Err(e) = invoke_clippy_fix(&root, &touched, config) {
                    eprintln!(
                        "comply: clippy --fix failed for {}: {e:#}",
                        root.display()
                    );
                } else {
                    runs += 1;
                }
            }
            None => {
                for f in files_in_ws {
                    skipped.push(f.path.display().to_string());
                }
            }
        }
    }

    if !skipped.is_empty() {
        eprintln!(
            "comply: clippy --fix skipped {} loose file(s) — no Cargo.toml \
             in any ancestor: {}",
            skipped.len(),
            skipped.join(", ")
        );
    }

    Ok(runs)
}

/// Walk up parents from each file looking for the nearest Cargo.toml.
/// Files outside any workspace land under the `None` key.
fn group_by_workspace<'a>(
    files: &[&'a SourceFile],
) -> std::collections::HashMap<Option<PathBuf>, Vec<&'a SourceFile>> {
    let mut out: std::collections::HashMap<Option<PathBuf>, Vec<&'a SourceFile>> =
        std::collections::HashMap::new();
    for f in files {
        let root = find_workspace_root(&f.path);
        out.entry(root).or_default().push(*f);
    }
    out
}

fn find_workspace_root(file: &Path) -> Option<PathBuf> {
    let mut cur = file.parent()?.to_path_buf();
    loop {
        if cur.join("Cargo.toml").is_file() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

/// Spawn `cargo clippy --fix` for one workspace.
///
/// `_touched` is currently unused — clippy --fix runs on the entire
/// crate, not on a per-file basis, so we can't restrict it to the
/// files comply was asked about. We accept the broader edit scope
/// because the alternative (no auto-fix at all on Rust) is worse.
fn invoke_clippy_fix(
    workspace: &Path,
    _touched: &HashSet<&Path>,
    config: &Config,
) -> Result<()> {
    let manifest = workspace.join("Cargo.toml");
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--fix",
        "--allow-dirty",
        "--allow-staged",
        "--quiet",
        "--manifest-path",
    ]);
    cmd.arg(&manifest);
    cmd.arg("--");

    // Forward the same per-rule `-W`/`-A` flags the lint mode would,
    // so a `[rules."clippy::xxx"] enabled = true` knob in comply.toml
    // also drives the fixer.
    let bindings = crate::rules::collect_clippy_bindings();
    for (lint, _, _) in &bindings {
        cmd.arg(format!("-W{lint}"));
    }
    for (rule_id, rule) in config.iter_rules() {
        if rule_id.starts_with("clippy::") {
            if rule.enabled == Some(true) {
                cmd.arg(format!("-W{rule_id}"));
            }
            if rule.disabled == Some(true) {
                cmd.arg(format!("-A{rule_id}"));
            }
        }
    }

    let status = cmd
        .status()
        .context("failed to invoke `cargo clippy --fix`")?;
    if !status.success() && status.code() != Some(0) && status.code() != Some(101) {
        anyhow::bail!("cargo clippy --fix exited with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn find_workspace_root_finds_immediate_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.0.0\"").unwrap();
        let src = tmp.path().join("src");
        fs::create_dir(&src).unwrap();
        let file = src.join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();
        assert_eq!(find_workspace_root(&file), Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn group_by_workspace_buckets_files_correctly() {
        let tmp_a = TempDir::new().unwrap();
        let tmp_b = TempDir::new().unwrap();
        fs::write(tmp_a.path().join("Cargo.toml"), "[package]\nname=\"a\"\nversion=\"0.0.0\"").unwrap();
        fs::write(tmp_b.path().join("Cargo.toml"), "[package]\nname=\"b\"\nversion=\"0.0.0\"").unwrap();
        let file_a = tmp_a.path().join("src/main.rs");
        let file_b = tmp_b.path().join("src/main.rs");
        fs::create_dir_all(file_a.parent().unwrap()).unwrap();
        fs::create_dir_all(file_b.parent().unwrap()).unwrap();
        fs::write(&file_a, "fn main() {}").unwrap();
        fs::write(&file_b, "fn main() {}").unwrap();

        let sf_a = SourceFile { path: file_a, language: Language::Rust };
        let sf_b = SourceFile { path: file_b, language: Language::Rust };
        let groups = group_by_workspace(&[&sf_a, &sf_b]);
        assert_eq!(groups.len(), 2);
    }
}
