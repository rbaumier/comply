//! cargo-modules subprocess — orphan Rust source files.
//!
//! Why this lives in Comply: a `.rs` file that no `mod` declaration loads
//! is dead code with worse properties than a regular orphan — rust-analyzer
//! still sees it, search tools index it, and grep returns matches that
//! point nowhere. cargo-modules
//! (https://github.com/regexident/cargo-modules) crawls the module tree
//! and lists every file the compiler can't reach.
//!
//! How it works:
//! 1. `is_available()` probes `cargo modules --version`. Cached.
//! 2. `lint_files()` finds the unique workspace roots (the nearest
//!    `Cargo.toml` ancestor) and runs:
//!
//!        cargo modules orphans --package <name> [--lib | --bin <name>] \
//!            --manifest-path <root>/Cargo.toml
//!
//!    cargo-modules resolves the workspace from the manifest.
//!    `--manifest-path` alone leaves the member unnamed.
//!    It then refuses to guess which one to walk.
//!    So every scan names its package.
//!    It walks one target and refuses a package with several.
//!    Such a package is walked target by target.
//!    `cargo metadata` lists them, and the scans are merged.
//!    cargo-modules outputs colored text (no JSON reporter as of v0.25),
//!    so we strip ANSI escapes and parse the `warning: orphaned module
//!    'X' at <path>` lines via a small regex.
//! 3. Each parsed orphan becomes one Comply diagnostic on the offending
//!    file path.

use anyhow::{Context, Result};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::SourceFile;
use crate::runner_helpers;

pub const RULE_ID: &str = "rust-orphan-module";

pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| runner_helpers::probe_binary("cargo", &["modules", "--version"]))
}

#[must_use = "diagnostics from cargo-modules must be reported"]
pub fn lint_files(files: &[&SourceFile]) -> Result<Vec<Diagnostic>> {
    if files.is_empty() {
        return Ok(vec![]);
    }
    let mut diagnostics = Vec::new();
    // Per-package on purpose: `cargo modules orphans` walks a single crate's
    // module tree and has no `--workspace` mode, so we group by the nearest
    // manifest (the member) rather than the workspace root the way the clippy
    // and shear runners do.
    for workspace in runner_helpers::collect_unique_roots(files, "Cargo.toml") {
        diagnostics.extend(scan_workspace(&workspace)?);
    }
    Ok(diagnostics)
}

fn scan_workspace(workspace: &Path) -> Result<Vec<Diagnostic>> {
    let targets = targets_of(workspace)?;
    // A virtual manifest declares no package to name.
    if targets.is_empty() {
        return scan_target(workspace, &[]);
    }
    if let [only] = targets.as_slice() {
        return scan_target(workspace, &only.selector);
    }
    // Parallel: each scan loads the whole crate graph. About 17 s on Qualneo.
    let scans = targets
        .par_iter()
        .map(|target| scan_target(workspace, &target.selector))
        .collect::<Result<Vec<_>>>()?;
    Ok(merge_scans(&targets, scans))
}

/// One lib or bin target of a package.
struct Target {
    /// The flags that walk this target of this package.
    /// The package is named because the manifest cannot.
    /// A member's manifest resolves to its whole workspace.
    selector: Vec<String>,
    /// The target's crate root file.
    root: PathBuf,
}

#[derive(Debug, Deserialize)]
// comply-ignore: rust-serde-deny-unknown-fields — cargo owns the `cargo metadata` schema.
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
// comply-ignore: rust-serde-deny-unknown-fields — same contract as `Metadata`.
struct Package {
    manifest_path: PathBuf,
    name: String,
    targets: Vec<RawTarget>,
}

#[derive(Debug, Deserialize)]
// comply-ignore: rust-serde-deny-unknown-fields — same contract as `Metadata`.
struct RawTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

/// The lib and bin targets of the package in `workspace`.
/// Empty for a virtual manifest: it declares no package.
fn targets_of(workspace: &Path) -> Result<Vec<Target>> {
    let manifest = workspace.join("Cargo.toml");
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .with_context(|| format!("failed to invoke `cargo metadata` in {}", workspace.display()))?;
    if !output.status.success() {
        return Err(crate::toolchain::ToolchainError::Unusable {
            what: format!(
                "cargo metadata failed in {}, so its targets are unknown and no module tree \
                 was walked. Repair the crate, then re-run comply.\n{}",
                workspace.display(),
                quote(&String::from_utf8_lossy(&output.stderr))
            ),
        }
        .into());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("unreadable `cargo metadata` output in {}", workspace.display()))?;
    let Some(package) = metadata.packages.into_iter().find(|p| p.manifest_path == manifest) else {
        return Ok(vec![]);
    };
    let Package { name, targets, .. } = package;
    let selected = ["--package".to_owned(), name];
    Ok(targets
        .into_iter()
        .filter_map(|raw| {
            let target = if raw.kind.iter().any(|k| k == "bin") {
                vec!["--bin".to_owned(), raw.name]
            } else if raw.kind.iter().any(|k| k.ends_with("lib") || k == "proc-macro") {
                vec!["--lib".to_owned()]
            } else {
                return None;
            };
            let mut selector = selected.to_vec();
            selector.extend(target);
            Some(Target { selector, root: raw.src_path })
        })
        .collect())
}

/// Each scan judges a directory against its own crate root.
/// Roots sharing a directory must all report an orphan.
/// A crate root is never an orphan.
fn merge_scans(targets: &[Target], scans: Vec<Vec<Diagnostic>>) -> Vec<Diagnostic> {
    let reported: Vec<FxHashSet<Arc<Path>>> =
        scans.iter().map(|scan| scan.iter().map(|d| d.path.clone()).collect()).collect();
    let mut kept = FxHashSet::default();
    scans
        .into_iter()
        .flatten()
        .filter(|orphan| {
            let dir = candidate_dir(&orphan.path);
            let unanimous = targets
                .iter()
                .zip(&reported)
                .filter(|(target, _)| module_dir(&target.root).as_deref() == dir)
                .all(|(_, paths)| paths.contains(&orphan.path));
            unanimous
                && !targets.iter().any(|target| *target.root == *orphan.path)
                && kept.insert(orphan.path.clone())
        })
        .collect()
}

/// The directory cargo-modules listed `file` from.
/// A `x/mod.rs` module is entry `x` of its grandparent.
fn candidate_dir(file: &Path) -> Option<&Path> {
    let parent = file.parent()?;
    if file.file_name().is_some_and(|name| name == "mod.rs") { parent.parent() } else { Some(parent) }
}

/// The directory a module rooted at `file` declares from.
/// `lib.rs`, `main.rs` and `mod.rs` own their directory.
/// `x.rs` owns `x/`.
fn module_dir(file: &Path) -> Option<PathBuf> {
    let parent = file.parent()?;
    let stem = file.file_stem()?;
    if ["lib", "main", "mod"].iter().any(|own| stem == *own) {
        Some(parent.to_path_buf())
    } else {
        Some(parent.join(stem))
    }
}

/// One `cargo modules orphans` run.
/// `selector` names the package and its target.
/// Empty only for a virtual manifest.
fn scan_target(workspace: &Path, selector: &[String]) -> Result<Vec<Diagnostic>> {
    let output = Command::new("cargo")
        .args(["modules", "orphans"])
        .args(selector)
        .arg("--manifest-path")
        .arg(workspace.join("Cargo.toml"))
        .output()
        .with_context(|| {
            format!(
                "failed to invoke `cargo modules orphans` in {}",
                workspace.display()
            )
        })?;
    // cargo-modules writes its rustc-style warnings to STDOUT (not stderr,
    // contrary to most cargo subcommands) and exits non-zero when orphans
    // exist. A run that fails before walking anything prints on stderr
    // instead, so that is where its reason is read from.
    let said = if output.stdout.is_empty() { &output.stderr } else { &output.stdout };
    let text = String::from_utf8_lossy(said).into_owned();
    read_orphans(&text, output.status.success(), workspace)
}

/// How many lines of the tool's own words a refusal quotes.
const QUOTED_LINES: usize = 5;

/// The orphans `said` names, or the refusal of a scan that read nothing.
///
/// `cargo modules orphans` exits non-zero in two opposite cases: when it found
/// orphans, and when it could not walk the module tree at all. The exit code was
/// ignored, so the second case returned an empty list that read as "no orphan in
/// this crate" — a verdict on a tree nobody walked. A failure that names an orphan
/// is a finding; a failure that names none is a refusal.
fn read_orphans(said: &str, succeeded: bool, workspace: &Path) -> Result<Vec<Diagnostic>> {
    let orphans = parse_orphans(said, workspace);
    if orphans.is_empty() && !succeeded {
        return Err(crate::toolchain::ToolchainError::Unusable {
            what: format!(
                "cargo modules orphans failed in {} without naming one orphan; no module tree \
                 was walked. Repair the crate, then re-run comply.\n{}",
                workspace.display(),
                quote(said)
            ),
        }
        .into());
    }
    Ok(orphans)
}

/// A tool's first lines, ANSI stripped, for a refusal.
fn quote(said: &str) -> String {
    let detail = strip_ansi(said).lines().take(QUOTED_LINES).collect::<Vec<_>>().join("\n");
    if detail.trim().is_empty() { "(no output)".to_owned() } else { detail.trim().to_owned() }
}

/// Strip ANSI color escapes and pull `at <path>` snippets out of the
/// `warning: orphaned module ... at <path>` lines.
fn parse_orphans(text: &str, workspace: &Path) -> Vec<Diagnostic> {
    let stripped = strip_ansi(text);
    let mut diagnostics = Vec::new();
    for line in stripped.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("warning: orphaned module") else {
            continue;
        };
        // The line shape is: ` 'name' at <path>`. We pull both segments.
        let Some(at_pos) = rest.find(" at ") else {
            continue;
        };
        // cargo-modules formats the module name with backticks: `name`.
        // Strip both backticks AND apostrophes so the diagnostic message
        // doesn't render as `` `name` ``.
        let module_part = rest[..at_pos]
            .trim()
            .trim_matches(|c: char| c == '`' || c == '\'')
            .to_string();
        let path_part = rest[at_pos + 4..].trim().to_string();
        let absolute = workspace.join(&path_part);
        diagnostics.push(Diagnostic {
            path: absolute.into(),
            line: 1,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "Orphan module `{module_part}` — no `mod {module_part};` declaration \
                 loads this file. Either declare it from the parent module or delete \
                 the file. Orphan files are dead code that grep still finds."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
    diagnostics
}

/// Minimal ANSI escape stripper — drops `ESC [ ... letter` sequences.
/// We don't need a full terminal emulator; cargo-modules only emits SGR
/// codes (color/format) and these always end in a single ASCII letter.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.next() != Some('[') {
                continue;
            }
            // Skip until terminator (an ASCII letter).
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_escapes() {
        let input = "\x1b[1;31mhello\x1b[0m world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn parses_orphan_line() {
        let text = "warning: orphaned module 'rust' at src/rules/foo/rust.rs\n  --> ...";
        let diagnostics = parse_orphans(text, Path::new("/proj"));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].path.ends_with("src/rules/foo/rust.rs"));
        assert!(diagnostics[0].message.contains("rust"));
    }

    /// `cargo modules orphans` exits 1 both when it found orphans and when
    /// it could not scan at all. The exit code was ignored, so a crate it
    /// never walked yielded an empty list that read as "no orphan here".
    #[test]
    fn a_failed_scan_that_found_nothing_is_refused() {
        let ws = Path::new("/proj");
        let found = "warning: orphaned module 'rust' at src/rules/foo/rust.rs";
        assert!(
            read_orphans(found, false, ws).is_ok_and(|d| d.len() == 1),
            "exit 1 with an orphan is how a finding is reported"
        );
        assert!(
            read_orphans("No orphans found.", true, ws).is_ok_and(|d| d.is_empty()),
            "exit 0 with nothing parsed is a genuinely clean crate"
        );
        assert!(
            read_orphans("error: failed to parse manifest", false, ws).is_err(),
            "exit 1 with nothing parsed is a crate that was never walked"
        );
    }

    /// A Qualneo-shaped package with a lib and three bins.
    /// `main.rs` declares a module of its own.
    /// Not one file of it is an orphan.
    const LIB_AND_BINS: &[(&str, &str)] = &[
        (
            "Cargo.toml",
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [[bin]]\nname = \"worker\"\npath = \"src/worker.rs\"\n",
        ),
        ("src/lib.rs", "pub mod used;\n"),
        ("src/used.rs", "pub fn f() {}\n"),
        ("src/main.rs", "mod helper;\nfn main() { helper::g(); }\n"),
        ("src/helper.rs", "pub fn g() {}\n"),
        ("src/worker.rs", "fn main() {}\n"),
        ("src/bin/tool.rs", "fn main() {}\n"),
    ];

    /// Writes `files` into a fresh directory.
    /// Returns it with its canonical path, as production does.
    fn package(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical temp dir");
        for (path, content) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent dir")).expect("source dirs");
            std::fs::write(file, content).expect("source file");
        }
        (dir, root)
    }

    /// `cargo modules orphans` refuses a package with several targets.
    /// Each target is walked on its own.
    /// A module one target declares is no orphan.
    #[test]
    fn a_package_with_a_lib_and_bins_is_walked_target_by_target() {
        let (_dir, root) = package(LIB_AND_BINS);
        let found = scan_workspace(&root).map_err(|err| format!("{err:#}"));
        assert_eq!(found.map(|d| d.len()), Ok(0));
    }

    #[test]
    fn an_orphan_beside_a_bin_is_reported() {
        let mut files = LIB_AND_BINS.to_vec();
        files.push(("src/bin/tool/lost.rs", "pub fn x() {}\n"));
        files.push(("src/stray.rs", "pub fn y() {}\n"));
        let (_dir, root) = package(&files);
        let found = scan_workspace(&root).map_err(|err| format!("{err:#}"));
        let paths = found.map(|d| {
            let mut paths: Vec<_> =
                d.iter().map(|d| d.path.strip_prefix(&root).ok().map(Path::to_path_buf)).collect();
            paths.sort();
            paths
        });
        assert_eq!(
            paths,
            Ok(vec![Some("src/bin/tool/lost.rs".into()), Some("src/stray.rs".into())])
        );
    }

    /// A workspace of two members, `alpha` carrying a lib alone.
    const WORKSPACE: &[(&str, &str)] = &[
        ("Cargo.toml", "[workspace]\nmembers = [\"alpha\", \"beta\"]\nresolver = \"2\"\n"),
        (
            "alpha/Cargo.toml",
            "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("alpha/src/lib.rs", "pub mod used;\n"),
        ("alpha/src/used.rs", "pub fn f() {}\n"),
        (
            "beta/Cargo.toml",
            "[package]\nname = \"beta\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("beta/src/lib.rs", "pub fn g() {}\n"),
    ];

    /// cargo-modules resolves the workspace from the manifest it is handed.
    /// It then refuses to guess which member to walk.
    /// A scan must name the member, single target or not.
    #[test]
    fn a_single_target_member_of_a_workspace_is_walked_not_refused() {
        let (_dir, root) = package(WORKSPACE);
        let found = scan_workspace(&root.join("alpha")).map_err(|err| format!("{err:#}"));
        assert_eq!(found.map(|d| d.len()), Ok(0));
    }

    /// Same workspace, with a lib and a bin.
    /// Naming the target alone does not name the member.
    #[test]
    fn an_orphan_in_a_multi_target_member_of_a_workspace_is_reported() {
        let mut files = WORKSPACE.to_vec();
        files.push(("alpha/src/main.rs", "fn main() {}\n"));
        files.push(("alpha/src/stray.rs", "pub fn y() {}\n"));
        let (_dir, root) = package(&files);
        let member = root.join("alpha");
        let found = scan_workspace(&member).map_err(|err| format!("{err:#}"));
        let paths = found.map(|d| {
            let mut paths: Vec<_> =
                d.iter().map(|d| d.path.strip_prefix(&member).ok().map(Path::to_path_buf)).collect();
            paths.sort();
            paths
        });
        assert_eq!(paths, Ok(vec![Some("src/stray.rs".into())]));
    }

    #[test]
    fn ignores_non_orphan_lines() {
        let text = "Compiling foo\nFinished dev profile\n";
        assert!(parse_orphans(text, Path::new("/proj")).is_empty());
    }
}
