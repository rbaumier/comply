//! `--fix` runs the oxlint and clippy fixers, then comply's own.
//! Loose `.rs` files outside a Cargo workspace are skipped.

use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};
use std::ops::Range;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::files::{Language, SourceFile};
use crate::rules::{
    copy_apostrophe_style, french_nbsp_before_punctuation, no_raw_invisible_character,
};

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
        match run_oxlint_fix(&ts_files, config) {
            Ok(()) => runs += 1,
            Err(e) => eprintln!("comply: oxlint --fix failed: {e:#}"),
        }
    }

    if !rs_files.is_empty() && clippy_available() {
        runs += run_clippy_fix(&rs_files, config)?;
    }

    Ok(runs)
}

/// Rewrites copy typography findings on disk, then drops them.
/// Returns how many it rewrote. Paths resolve against `root`.
/// A file that fails stays untouched and reported.
pub fn rewrite_copy(diagnostics: &mut Vec<Diagnostic>, root: &Path) -> usize {
    let mut by_path: FxHashMap<Arc<Path>, Vec<usize>> = FxHashMap::default();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let is_copy = [
            french_nbsp_before_punctuation::META.id,
            no_raw_invisible_character::META.id,
            copy_apostrophe_style::META.id,
        ]
        .contains(&diagnostic.rule_id.as_ref());
        if is_copy && remedy(&diagnostic.message).is_some() {
            by_path
                .entry(Arc::clone(&diagnostic.path))
                .or_default()
                .push(index);
        }
    }
    let mut rewritten = FxHashSet::default();
    for (path, indices) in by_path {
        let file = root.join(&path);
        let findings: Vec<&Diagnostic> = indices.iter().map(|&i| &diagnostics[i]).collect();
        match rewrite_file(&file, &findings) {
            Ok(()) => rewritten.extend(indices),
            Err(e) => eprintln!("comply: left {} untouched: {e:#}", file.display()),
        }
    }
    let kept = std::mem::take(diagnostics)
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !rewritten.contains(index))
        .map(|(_, diagnostic)| diagnostic);
    diagnostics.extend(kept);
    rewritten.len()
}

/// Applies every finding's edit to `file`, or none.
fn rewrite_file(file: &Path, findings: &[&Diagnostic]) -> Result<()> {
    let mut text = std::fs::read_to_string(file).context("cannot read it")?;
    let mut edits = findings
        .iter()
        .map(|finding| edit(&text, finding))
        .collect::<Option<Vec<_>>>()
        .context("a finding no longer matches its text")?;
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in edits {
        text.replace_range(range, replacement);
    }
    anyhow::ensure!(parses(file, &text), "the fixed text does not parse");
    std::fs::write(file, text).context("cannot write it")
}

/// The byte range a finding rewrites, and its text.
fn edit<'a>(text: &str, finding: &'a Diagnostic) -> Option<(Range<usize>, &'a str)> {
    let (start, len) = finding.span?;
    let replacement = remedy(&finding.message)?;
    let range = if finding.rule_id == french_nbsp_before_punctuation::META.id {
        // The escape goes before the mark, replacing a plain space.
        let has_space = text.get(..start)?.ends_with(' ');
        let space_start = start.checked_sub(usize::from(has_space))?;
        space_start..start
    } else {
        start..start.checked_add(len)?
    };
    text.get(range.clone())?;
    Some((range, replacement))
}

/// The remedy a message names after `write \``.
fn remedy(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once("write `")?;
    rest.split_once('`').map(|(replacement, _)| replacement)
}

/// True when `text` parses as the language of `path`.
fn parses(path: &Path, text: &str) -> bool {
    match Language::from_path(path) {
        Some(Language::Rust) => {
            let mut parser = tree_sitter::Parser::new();
            crate::parsing::parse_with_grammar(&mut parser, Language::Rust, text.as_bytes())
                .is_some_and(|tree| !tree.root_node().has_error())
        }
        // oxc, since tree-sitter's TS grammar rejects JSX in a `.js`.
        Some(language) if language.is_typescript_family() => {
            let allocator = oxc_allocator::Allocator::default();
            let source_type = crate::oxc_helpers::source_type_for_path(path);
            let parsed = oxc_parser::Parser::new(&allocator, text, source_type).parse();
            parsed.errors.is_empty() && !parsed.panicked
        }
        _ => true,
    }
}

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
fn run_oxlint_fix(files: &[&SourceFile], config: &Config) -> Result<()> {
    let bindings = crate::rules::collect_oxlint_bindings();
    let rule_entries: Vec<crate::oxlint_config::RuleEntry> = bindings
        .iter()
        .map(|(key, _, sev)| (*key, *sev, crate::oxlint::options_for(key, config)))
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
/// distinct workspace touched by the input files, on the packages that
/// own them. Returns the successful invocation count.
fn run_clippy_fix(files: &[&SourceFile], config: &Config) -> Result<usize> {
    let mut runs = 0;
    let workspaces = crate::clippy::group_by_workspace(files);
    let mut skipped: Vec<String> = Vec::new();

    for (workspace, files_in_ws) in workspaces {
        match workspace {
            Some(root) => {
                let packages = crate::runner_helpers::cargo_package_args(&root, &files_in_ws);
                if let Err(e) = invoke_clippy_fix(&root, &packages, config) {
                    eprintln!("comply: clippy --fix failed for {}: {e:#}", root.display());
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

/// Spawn `cargo clippy --fix` for one workspace on `packages`
/// (`-p` flags from `cargo_package_args`). Clippy fixes whole crates,
/// so other files of those packages may be edited too.
fn invoke_clippy_fix(workspace: &Path, packages: &[String], config: &Config) -> Result<()> {
    let manifest = workspace.join("Cargo.toml");
    let mut cmd = Command::new("cargo");
    cmd.arg("clippy").args(packages);
    cmd.args([
        "--fix",
        // clippy refuses dirty or staged trees without these.
        "--all-targets",
        "--allow-dirty",
        "--allow-staged",
        "--quiet",
        "--manifest-path",
    ]);
    cmd.arg(&manifest);
    let (_clippy_conf_guard, clippy_conf_dir) = crate::clippy::config_writer::materialize(config)?;
    let (key, val) = crate::clippy::config_writer::env_var_for_dir(&clippy_conf_dir);
    cmd.env(key, val);
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
