//! Regenerate `src/clippy/all_lints.rs` and `src/clippy/all_args.rs`
//! from the live `cargo clippy -- -W help` output and the upstream
//! clippy.toml schema.
//!
//! Run from the repo root:
//!     cargo run --bin regen-clippy-lints
//!
//! Outputs:
//!   src/clippy/all_lints.rs   — every (lint_name, default_level) pair
//!   src/clippy/all_args.rs    — known threshold-bearing lints + their
//!                                clippy.toml configuration keys
//!
//! Why a binary in the comply crate (not a separate xtask):
//!   - The crate has a single existing binary (`comply`); cargo
//!     auto-discovers `src/bin/*.rs` as additional binaries with no
//!     workspace plumbing.
//!   - The script needs nothing from the comply library — it shells
//!     out to cargo, parses stdout, and writes files. No shared types,
//!     so the duplication cost is zero.
//!
//! Why a script and not a build.rs:
//!   - The output is committed to git so consumers of comply can
//!     inspect (and grep) every clippy lint comply knows about.
//!   - It only needs to be regenerated when the toolchain bumps clippy.
//!     Running it on every build would slow down incremental compiles
//!     for no benefit and would force every consumer to have clippy
//!     installed at build time.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

// Step 1: enumerate every clippy lint via `cargo clippy -- -W help`.

/// Capture stdout+stderr from `cargo clippy -- -W help`. Cargo emits the
/// lint dump on stderr; we capture both to be safe across cargo versions.
fn run_clippy_help() -> anyhow::Result<String> {
    let output = Command::new("cargo")
        .args(["clippy", "--quiet", "--", "-W", "help"])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to invoke `cargo clippy`: {e}"))?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(combined)
}

/// Parse `cargo clippy -W help` text. Lines look like:
///
///     clippy::name-with-dashes  warn  description text…
///
/// We turn the kebab-case name into snake_case (the form clippy emits
/// in its diagnostic JSON), drop duplicates, and sort by name.
fn parse_lints(text: &str) -> BTreeMap<String, &'static str> {
    let mut out: BTreeMap<String, &'static str> = BTreeMap::new();
    for line in text.lines() {
        // The first whitespace-separated token must start with `clippy::`.
        let trimmed = line.trim_start();
        if !trimmed.starts_with("clippy::") {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(kebab_name) = parts.next() else {
            continue;
        };
        let Some(level_word) = parts.next() else {
            continue;
        };
        // Levels are exactly the four cargo lint levels; anything else
        // is a stray line we don't care about.
        let level = match level_word {
            "allow" => "Allow",
            "warn" => "Warn",
            "deny" => "Deny",
            "forbid" => "Forbid",
            _ => continue,
        };
        let snake = kebab_name.replace('-', "_");
        out.insert(snake, level);
    }
    out
}

// Step 2: hardcoded list of threshold-bearing clippy lints + their
// clippy.toml configuration keys. Extracted from the clippy book at
// https://rust-lang.github.io/rust-clippy/master/index.html
//
// Format: (lint_name, clippy_toml_key, value_kind)
// `value_kind` is one of: "Int", "String", "Array".
//
// When the user puts `[rules."clippy::too_many_lines"] threshold = 30`
// in their comply.toml, the clippy module translates that into
// `too-many-lines-threshold = 30` in the generated clippy.toml.

const THRESHOLD_LINTS: &[(&str, &str, &str)] = &[
    // Numeric thresholds
    ("clippy::too_many_lines", "too-many-lines-threshold", "Int"),
    ("clippy::too_many_arguments", "too-many-arguments-threshold", "Int"),
    ("clippy::large_enum_variant", "enum-variant-size-threshold", "Int"),
    ("clippy::large_stack_arrays", "stack-size-threshold", "Int"),
    ("clippy::large_stack_frames", "stack-size-threshold", "Int"),
    ("clippy::large_types_passed_by_value", "pass-by-value-size-limit", "Int"),
    ("clippy::cognitive_complexity", "cognitive-complexity-threshold", "Int"),
    ("clippy::excessive_nesting", "excessive-nesting-threshold", "Int"),
    ("clippy::min_ident_chars", "min-ident-chars-threshold", "Int"),
    ("clippy::single_char_lifetime_names", "single-char-binding-names-threshold", "Int"),
    ("clippy::struct_excessive_bools", "max-struct-bools", "Int"),
    ("clippy::fn_params_excessive_bools", "max-fn-params-bools", "Int"),
    ("clippy::trivial_copy_pass_by_ref", "trivial-copy-size-limit", "Int"),
    ("clippy::type_complexity", "type-complexity-threshold", "Int"),
    ("clippy::unreadable_literal", "unreadable-literal-lint-fractions", "Int"),
    ("clippy::vec_box", "vec-box-size-threshold", "Int"),
    ("clippy::verbose_bit_mask", "verbose-bit-mask-threshold", "Int"),
    ("clippy::missing_docs_in_private_items", "missing-docs-in-crate-items", "Int"),
    // Array thresholds (allowlists)
    ("clippy::disallowed_names", "disallowed-names", "Array"),
    ("clippy::doc_markdown", "doc-valid-idents", "Array"),
    ("clippy::min_ident_chars", "allowed-idents-below-min-chars", "Array"),
];

// Step 3: render the Rust files.

const ALL_LINTS_PROLOGUE: &str = "\
//! Auto-generated by `cargo run --bin regen-clippy-lints` — DO NOT EDIT BY HAND.
//!
//! Snapshot of every lint clippy advertises via `cargo clippy -- -W help`.
//! Comply uses this list so a project's `comply.toml` can disable, enable,
//! or override the severity of any clippy lint without comply having to
//! ship a per-lint binding for each one.
//!
//! Regenerate when the toolchain bumps clippy:
//!     cargo run --bin regen-clippy-lints

/// Default level a clippy lint fires at when no `-W` / `-A` / `-D` flag
/// modifies it. Mirrors clippy's `Allow` / `Warn` / `Deny` / `Forbid`.
#[allow(dead_code)] // Forbid currently unused; keep the variant for future-proofing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClippyDefaultLevel {
    Allow,
    Warn,
    Deny,
    Forbid,
}

/// Every clippy lint, indexed by snake_case name. The names match what
/// clippy emits in its diagnostic JSON output (`clippy::unwrap_used`,
/// `clippy::needless_borrow`, …) so a `comply.toml` entry like
/// `[rules.\"clippy::needless_borrow\"] disabled = true` Works.
pub const ALL_CLIPPY_LINTS: &[(&str, ClippyDefaultLevel)] = &[
";

const ALL_ARGS_PROLOGUE: &str = "\
//! Auto-generated by `cargo run --bin regen-clippy-lints` — DO NOT EDIT BY HAND.
//!
//! Mapping from clippy lint name to its `clippy.toml` configuration key.
//! Used by `crate::clippy::config_writer` to translate per-lint
//! thresholds in `comply.toml` (e.g. `[rules.\"clippy::too_many_lines\"]
//! threshold = 50`) into the right `clippy.toml` line for the temporary
//! config file we point clippy at.
//!
//! Most lints with thresholds use the comply key `threshold`; lints with
//! list-shaped values (`disallowed_names`, etc.) use the comply key
//! `values`. The `kind` field tells the writer which TOML shape to emit.

#[allow(dead_code)] // String is unused today; keep it for future schemas.
#[derive(Debug, Clone, Copy)]
pub enum ArgKind {
    Int,
    String,
    Array,
}

pub struct ClippyArg {
    pub lint: &'static str,
    pub clippy_toml_key: &'static str,
    pub kind: ArgKind,
}

pub const CLIPPY_THRESHOLD_LINTS: &[ClippyArg] = &[
";

fn render_all_lints(lints: &BTreeMap<String, &'static str>) -> String {
    let mut out = String::from(ALL_LINTS_PROLOGUE);
    for (name, level) in lints {
        out.push_str(&format!(
            "    (\"{name}\", ClippyDefaultLevel::{level}),\n"
        ));
    }
    out.push_str("];\n");
    out
}

fn render_all_args() -> String {
    let mut out = String::from(ALL_ARGS_PROLOGUE);
    for (lint, key, kind) in THRESHOLD_LINTS {
        out.push_str(&format!(
            "    ClippyArg {{ lint: \"{lint}\", clippy_toml_key: \"{key}\", kind: ArgKind::{kind} }},\n"
        ));
    }
    out.push_str("];\n");
    out
}

// Main

fn repo_root_from_invocation() -> anyhow::Result<PathBuf> {
    // The script is normally invoked via `cargo run --bin regen-clippy-lints`
    // from the repo root, so the cwd is already correct. We still
    // verify by checking for `Cargo.toml` and `src/clippy`.
    let cwd = env::current_dir()
        .map_err(|e| anyhow::anyhow!("failed to read current_dir: {e}"))?;
    if !cwd.join("Cargo.toml").is_file() || !cwd.join("src/clippy").is_dir() {
        anyhow::bail!(
            "expected to run from comply repo root (got {})",
            cwd.display()
        );
    }
    Ok(cwd)
}

fn write_atomic(path: &Path, contents: &str) -> anyhow::Result<()> {
    fs::write(path, contents)
        .map_err(|e| anyhow::anyhow!("failed to write {}: {e}", path.display()))
}

fn run() -> anyhow::Result<()> {
    let root = repo_root_from_invocation()?;
    let target_dir = root.join("src/clippy");

    println!("Running cargo clippy -- -W help …");
    let text = run_clippy_help()?;
    let lints = parse_lints(&text);
    println!("  found {} clippy lints", lints.len());
    if lints.is_empty() {
        anyhow::bail!("parser found zero lints — check the cargo clippy output format");
    }

    let all_lints_path = target_dir.join("all_lints.rs");
    let all_args_path = target_dir.join("all_args.rs");

    write_atomic(&all_lints_path, &render_all_lints(&lints))?;
    write_atomic(&all_args_path, &render_all_args())?;

    println!("  wrote {}", all_lints_path.display());
    println!("  wrote {}", all_args_path.display());
    println!("Done.");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("regen-clippy-lints: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clippy_help_excerpt() {
        let sample = "\
            Lint checks provided by plugins loaded by this crate:\n\
            \n\
                                    name  default  meaning\n\
                                    ----  -------  -------\n\
                clippy::absurd_extreme_comparisons    deny    foo\n\
                          clippy::needless-borrow    warn    bar\n\
                            clippy::indexing-slicing    allow   baz\n\
        ";
        let lints = parse_lints(sample);
        assert_eq!(
            lints.get("clippy::absurd_extreme_comparisons").copied(),
            Some("Deny")
        );
        assert_eq!(
            lints.get("clippy::needless_borrow").copied(),
            Some("Warn")
        );
        assert_eq!(
            lints.get("clippy::indexing_slicing").copied(),
            Some("Allow")
        );
    }

    #[test]
    fn ignores_lines_without_clippy_prefix() {
        let sample = "\
            random preamble\n\
            non-clippy-lint  warn  irrelevant\n\
            cargo: error\n\
        ";
        assert!(parse_lints(sample).is_empty());
    }

    #[test]
    fn deduplicates_lints_emitted_twice() {
        let sample = "\
            clippy::foo  warn  description one\n\
            clippy::foo  allow  description two\n\
        ";
        let lints = parse_lints(sample);
        assert_eq!(lints.len(), 1);
        // Second occurrence wins because BTreeMap::insert overwrites.
        assert_eq!(lints.get("clippy::foo").copied(), Some("Allow"));
    }
}
