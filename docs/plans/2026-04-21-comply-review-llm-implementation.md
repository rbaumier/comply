# `comply review` — LLM review pipeline implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-development (recommended) or plans skill to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `comply review`, a local-first, pre-push manual LLM code review subcommand that runs on git diffs, injects relevant user skills from `~/.claude/skills`, and produces a severity-ranked markdown report with blast-radius-adjusted scoring.

**Architecture:** New top-level subcommand orchestrating a multi-stage pipeline:
`diff parse → deterministic pre-filter → Haiku triage + skill selection → partial 1-hop dep graph + tree-sitter hunk widening → parallel Sonnet findings → score/dedup/filter → markdown|json output`. A backend abstraction lets us default to `ai-sdk-provider-claude-code` (α, zero API key) while leaving an opt-in path to `@ai-sdk/anthropic` direct (β, real prompt caching via env var `COMPLY_REVIEW_BACKEND=anthropic`).

**Tech Stack:** Rust (core pipeline, CLI, cache), tree-sitter (widening, dep graph, chunking), Bun + Vercel AI SDK (LLM worker), SQLite via `rusqlite` (cache, already used by `src/llm/cache.rs`), `oxc_resolver` for TS/JS module resolution.

**Authoritative design input:** All product decisions are locked in `docs/plans/2026-04-21-comply-review-llm-decision-log.md`. Do not relitigate; implement.

---

## Scope check

This is one pipeline with coherent invariants. Not splitting further.

## File structure

New code lives under `src/review/` (mirrors the existing `src/llm/` pattern but is a standalone subsystem — separate cache DB, separate worker, separate prompts).

**New Rust modules:**

| Path | Responsibility |
|---|---|
| `src/review/mod.rs` | Public entry `run(cli) -> Result<ExitCode>`; pipeline orchestration |
| `src/review/config.rs` | `[review]` section struct + defaults merge |
| `src/review/diff.rs` | Parse git diff → `ReviewFile { path, hunks, status }` |
| `src/review/prefilter.rs` | Deterministic skip (binaries, lockfiles, `.min.js`, `@generated`, pure renames, whitespace-only) |
| `src/review/skill.rs` | Load `~/.claude/skills`, parse frontmatter, whitelist enforcement |
| `src/review/triage.rs` | Haiku call — verdict + picked skill IDs |
| `src/review/widen.rs` | Tree-sitter widen hunks to enclosing function/class/impl |
| `src/review/dep_graph.rs` | Partial 1-hop dep graph (TS via `oxc_resolver`, Rust via manual mod tree) |
| `src/review/cross_file.rs` | Build ≤ 2k-token cross-file context (exported sigs + caller call-sites) |
| `src/review/chunk.rs` | Build chunks respecting 8k-token cap; split by top-level items if needed |
| `src/review/finding.rs` | `Finding`, `ReviewSeverity` (`Critical/High/Medium/Low/Nit`), `QualityScore` |
| `src/review/prompt.rs` | Build findings prompt (XML-ish: `<review_guidelines>` + `<file_under_review>` + `<cross_file_context>`) |
| `src/review/scoring.rs` | Blast-radius severity boost, dedup, threshold filter, max-findings cap |
| `src/review/backend.rs` | Backend enum (`ClaudeCode` / `Anthropic`) dispatched by env var |
| `src/review/worker.rs` | Spawn Bun `review-worker.ts`, send NDJSON, collect results |
| `src/review/cache.rs` | SQLite cache: schema, lookup, store, invalidation |
| `src/review/summary.rs` | Sonnet summary call with map-reduce path when diff > 15 files |
| `src/review/output/mod.rs` | Output dispatcher (`OutputFormat::Markdown` / `Json`) |
| `src/review/output/markdown.rs` | Human-readable report |
| `src/review/output/json.rs` | NDJSON machine format |
| `src/review/error.rs` | Review-specific error type |

**Modified Rust files:**

| Path | Change |
|---|---|
| `src/cli.rs` | Add `Command::Review { ... }` variant with flags |
| `src/main.rs` | Dispatch `Command::Review` to `review::run(cli)` |
| `src/config/defaults.toml` | Append `[review]` section |
| `src/config/mod.rs` | Add `review()` accessor returning `ReviewConfig` |

**New TS:**

| Path | Responsibility |
|---|---|
| `tools/review-worker.ts` | New worker — two modes: `triage` and `findings`; two backends: `claude-code` (default) and `anthropic` |
| `tools/package.json` | Add `@ai-sdk/anthropic` as optionalDependency |

**Tests (co-located `#[cfg(test)] mod tests`):** each module has inline tests. Integration test `tests/review_e2e.rs` runs the full pipeline with a worker stub.

---

## Branch & milestones

Work on `feat/comply-review` (create from `main`). Each milestone ships a usable, tested increment. Commit granularity: one commit per task. No squash — preserve the TDD trail.

| Milestone | Output | Tests required |
|---|---|---|
| M1 | `comply review` prints pre-filter results | prefilter rules, CLI parsing, diff adapter |
| M2 | Triage (verdict + skills) via worker | skill loading, triage JSON schema, backend dispatch |
| M3 | 1-hop dep graph + widened chunks | widening, resolver fixtures per language |
| M4 | Full findings + scoring + dedup | blast radius, dedup, threshold |
| M5 | Markdown + JSON output | snapshot tests |
| M6 | SQLite cache + failure modes | cache invalidation, partial result, 50-file cap |
| M7 | Tuned prompts, dogfooded, docs | 3 real PRs reviewed |

---

## M1 — CLI stub + diff parsing + deterministic pre-filter

**Goal:** `comply review [same diff flags as comply] --format markdown|json` runs end-to-end with no LLM calls. It parses the diff, applies deterministic pre-filter, prints surviving paths to stderr, exits 0.

This milestone is pure plumbing. No network, no LLM. It dérisque the CLI shape, diff adapter, and filter heuristics.

### Task 1.0: Bootstrap — verify and add all required dependencies

Before any code: inspect `Cargo.toml` + `tools/package.json` and stage every dep the subsequent tasks will need. Doing this in one task avoids mid-milestone `cargo build` failures.

**Files:**
- Modify: `Cargo.toml` (main + dev deps)
- Modify: `tools/package.json`

- [ ] **Step 1: Audit current dependencies**

Run:
```bash
grep -E '^(sha2|hex|thiserror|rusqlite|serde|serde_json|oxc_resolver|tree-sitter|temp-env|assert_cmd) ' Cargo.toml
grep -E '^(assert_cmd|tempfile)' Cargo.toml
cat tools/package.json
```
Record which of `sha2`, `hex`, `thiserror`, `rusqlite`, `serde`, `serde_json`, `oxc_resolver`, `tree-sitter-typescript`, `tree-sitter-rust`, `temp-env`, `assert_cmd`, `tempfile` are already present.

- [ ] **Step 2: Add missing Rust deps**

For each missing dep, `cargo add <name>` (runtime) or `cargo add --dev <name>` (test-only). Test-only: `temp-env`, `assert_cmd`, `tempfile`. Confirm `rusqlite` is listed (used by `src/llm/cache.rs`).

Expected ending state in `Cargo.toml`:
- Runtime: `sha2`, `hex`, `thiserror`, `rusqlite`, `serde` (with `derive`), `serde_json`, `oxc_resolver`, `tree-sitter`, `tree-sitter-typescript`, `tree-sitter-rust`
- Dev: `temp-env`, `assert_cmd`, `tempfile`

- [ ] **Step 3: Add `@ai-sdk/anthropic` as optional Bun dep**

```bash
cd tools && bun add -d '@ai-sdk/anthropic'   # dev dep so tree-shaken when unused
```

- [ ] **Step 4: Verify clean build**

```bash
cargo build
cargo nextest run --no-run
```

Expected: both succeed.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock tools/package.json tools/bun.lock
git commit -m "chore(review): stage all deps for the comply review subsystem"
```

### Task 1.1: Add `Command::Review` CLI variant

**Files:**
- Modify: `src/cli.rs` (append to `Command` enum, add `ReviewFormat` + `ReviewScanMode` helpers)

- [ ] **Step 1: Write the failing test**

Append to `src/cli.rs::tests`:

```rust
#[test]
fn review_subcommand_with_format_json_parses() {
    use clap::Parser;
    let cli = Cli::try_parse_from([
        "comply", "review", "--format", "json", "--last-commit",
    ])
    .expect("review --format json --last-commit should parse");
    match cli.command {
        Some(Command::Review { format, threshold, deep, no_cache, concurrency }) => {
            assert!(matches!(format, ReviewFormat::Json));
            assert_eq!(threshold, None);
            assert!(!deep);
            assert!(!no_cache);
            assert_eq!(concurrency, 5);
        }
        _ => panic!("expected Command::Review"),
    }
    assert!(cli.last_commit);
}
```

- [ ] **Step 2: Run test — verify it fails**

```
cargo nextest run -p comply --no-fail-fast cli::tests::review_subcommand_with_format_json_parses
```

Expected: compile error — `Command::Review` variant not defined.

- [ ] **Step 3: Implement the variant**

Add to `src/cli.rs` after the `Lsp` variant in `Command`:

```rust
/// Run an LLM-powered code review on a git diff. Local, manual,
/// pre-push. Uses your Claude Code subscription by default (set
/// `COMPLY_REVIEW_BACKEND=anthropic` + `ANTHROPIC_API_KEY` to switch
/// to the direct Anthropic SDK with prompt caching).
Review {
    /// Output format.
    #[arg(long, default_value = "markdown")]
    format: ReviewFormat,
    /// Minimum severity to report. Defaults to the value in
    /// comply.toml `[review] threshold` (itself defaulting to "medium").
    #[arg(long)]
    threshold: Option<ReviewSeverityArg>,
    /// Use Opus (instead of Sonnet) for findings. Slower, more expensive,
    /// only useful on dense reviews.
    #[arg(long)]
    deep: bool,
    /// Skip the cache (force re-evaluation of every chunk).
    #[arg(long)]
    no_cache: bool,
    /// Maximum concurrent Sonnet calls.
    #[arg(long, default_value = "5")]
    concurrency: usize,
},
```

Add the two `#[derive(ValueEnum, Debug, Clone, Copy)]` helpers at the bottom of the file:

```rust
#[derive(clap::ValueEnum, Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ReviewFormat { Markdown, Json }

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ReviewSeverityArg { Critical, High, Medium, Low, Nit }
```

- [ ] **Step 4: Run test — verify it passes**

```
cargo nextest run -p comply cli::tests::review_subcommand_with_format_json_parses
```

Expected: PASS.

- [ ] **Step 5: Commit**

```
git add src/cli.rs
git commit -m "feat(review): add comply review CLI subcommand skeleton"
```

### Task 1.2: Widen `fn run()` return type to `Result<ExitCode>`

**Files:**
- Modify: `src/main.rs` (change signature + all return sites)

**Why this lands in M1, not M5:** the current contract `fn run() -> Result<bool>` has only 2 outcomes — `Ok(true) → 1`, `Ok(false) → 0`, plus `Err → 2`. `comply review` needs to return exit 1 for findings AND reuse the existing exit 2 for system errors. Waiting until M5 forces us to hack Task 1.2's dispatch (tunneling via bool) and then rework it. Doing it now is cheaper and unblocks M5.

- [ ] **Step 1: Write the regression test**

Add to `tests/exit_codes.rs` (create if missing):

```rust
use assert_cmd::Command;

#[test]
fn existing_lint_clean_exits_0() {
    // Run comply on an empty tempdir — nothing to lint, should exit 0.
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("comply").unwrap()
        .arg(dir.path())
        .assert()
        .code(0);
}

#[test]
fn missing_path_exits_2() {
    Command::cargo_bin("comply").unwrap()
        .arg("/nonexistent/path/that/should/fail")
        .assert()
        .code(2);
}
```

- [ ] **Step 2: Widen the signature**

In `src/main.rs`, change:

```rust
// before:
fn run() -> Result<bool> { ... }

fn main() -> Result<()> {
    match run() {
        Ok(true) => std::process::exit(1),
        Ok(false) => std::process::exit(0),
        Err(e) => { eprintln!("{e}"); std::process::exit(2); }
    }
}
```

to:

```rust
// after:
use std::process::ExitCode;

fn run() -> Result<ExitCode> { ... }

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => { eprintln!("{e}"); ExitCode::from(2) }
    }
}
```

Walk every `return Ok(true)` → `return Ok(ExitCode::from(1))`, and every `return Ok(false)` / implicit tail `Ok(false)` → `Ok(ExitCode::from(0))`. Use `grep -n 'Ok(true)\|Ok(false)' src/main.rs` to enumerate. Leave `Err(...)` arms unchanged — anyhow's `?` still propagates to exit 2.

- [ ] **Step 3: Run the regression + full suite**

```
cargo nextest run --test exit_codes
cargo nextest run
cargo clippy --all --all-targets -- -D warnings
```

Expected: all pass (behavior unchanged, only the internal type changed).

- [ ] **Step 4: Commit**

```
git add src/main.rs tests/exit_codes.rs
git commit -m "refactor(main): widen run() return to ExitCode for review exit 1 routing"
```

### Task 1.3: Wire `Command::Review` dispatch + stub module

**Files:**
- Create: `src/review/mod.rs`
- Create: `src/review/error.rs`
- Modify: `src/main.rs` (register module + dispatch)

- [ ] **Step 1: Create `src/review/error.rs`**

```rust
//! Review-specific error type. Wraps anyhow::Error with a marker so
//! main.rs can decide to exit 2 (system error) vs exit 1 (findings).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("diff too large ({0} files NEEDS_REVIEW post-triage, cap is 50). Narrow the scan with --commit or --range.")]
    DiffTooLarge(usize),

    #[error("no backend available: {0}")]
    BackendUnavailable(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 2: Create `src/review/mod.rs`**

```rust
//! LLM-driven code review pipeline. See
//! `docs/plans/2026-04-21-comply-review-llm-decision-log.md` for the
//! locked design decisions.

pub mod error;

use crate::cli::Cli;
use anyhow::Result;
use std::process::ExitCode;

/// Entry point for `comply review`. Orchestrates the full pipeline.
pub fn run(_cli: &Cli) -> Result<ExitCode> {
    // M1 placeholder: echo that we got here.
    eprintln!("comply review: M1 stub — pipeline not wired yet");
    Ok(ExitCode::from(0))
}
```

- [ ] **Step 3: Register + dispatch in `src/main.rs`**

Add `mod review;` next to `mod llm;`.

Add branch in the `match cli.command` of `fn run()`:

```rust
Some(Command::Review { .. }) => review::run(&cli),
```

That's it. Since Task 1.2 already widened `run()` to return `Result<ExitCode>`, the ExitCode flows through cleanly — no bool tunneling, no comment justifying a hack.

- [ ] **Step 4: Manual smoke test**

```
cargo run -q -- review --last-commit
```

Expected stderr: `comply review: M1 stub — pipeline not wired yet`, exit 0.

- [ ] **Step 5: Commit**

```
git add src/review/ src/main.rs
git commit -m "feat(review): wire Command::Review to stub module"
```

### Task 1.4: Diff adapter — `ReviewFile` struct

**Files:**
- Create: `src/review/diff.rs`
- Modify: `src/review/mod.rs` (`pub mod diff;`)

Reuses `src/changed_lines.rs` for raw diff extraction but produces a review-oriented struct (diff hunks, not just line numbers).

- [ ] **Step 1: Write failing tests**

Create `src/review/diff.rs` (tests only for now):

```rust
//! Parse a git diff scoped by `ScanMode` into a list of `ReviewFile`
//! entries ready for the review pipeline. Each file carries its hunks
//! (with added/removed line content) so downstream stages can widen
//! via tree-sitter without re-running git.

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_single_added_line_hunk() {
        // NOTE: git emits pure additions as `@@ -0,0 +1 @@` (no `-0,0`
        // comma form, no `+1,1` either — git omits the count for single
        // lines). `git diff --unified=3 /dev/null file.ts` locally to
        // regenerate if this fixture ever needs updating.
        let unified = "\
diff --git a/a.ts b/a.ts
new file mode 100644
index 0000000..222
--- /dev/null
+++ b/a.ts
@@ -0,0 +1 @@
+const x = 1;
";
        let files = parse_unified(unified).unwrap();
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path, PathBuf::from("a.ts"));
        assert_eq!(f.hunks.len(), 1);
        assert_eq!(f.hunks[0].added_lines, vec![(1usize, "const x = 1;".into())]);
    }

    #[test]
    fn pure_rename_has_no_hunks() {
        let unified = "\
diff --git a/a.ts b/b.ts
similarity index 100%
rename from a.ts
rename to b.ts
";
        let files = parse_unified(unified).unwrap();
        assert_eq!(files.len(), 1);
        assert!(matches!(files[0].status, FileStatus::Renamed { .. }));
        assert!(files[0].hunks.is_empty());
    }

    #[test]
    fn parses_modified_hunk_with_context_lines() {
        // Realistic diff: one removed line, one added line, with 2
        // context lines on each side. Tests that context (` ` prefix)
        // is ignored, `-` lines go to removed, `+` lines go to added,
        // and line numbers track the new-file side correctly.
        let unified = "\
diff --git a/a.ts b/a.ts
index 111..222 100644
--- a/a.ts
+++ b/a.ts
@@ -10,5 +10,5 @@
 const header = true;
 const footer = false;
-const middle = 1;
+const middle = 42;
 const tail = 2;
 const end = 3;
";
        let files = parse_unified(unified).unwrap();
        assert_eq!(files.len(), 1);
        assert!(matches!(files[0].status, FileStatus::Modified));
        assert_eq!(files[0].hunks.len(), 1);
        let h = &files[0].hunks[0];
        assert_eq!(h.added_lines, vec![(12usize, "const middle = 42;".into())]);
        assert_eq!(h.removed_lines, vec![(12usize, "const middle = 1;".into())]);
    }

    #[test]
    fn parses_multiple_hunks_in_one_file() {
        let unified = "\
diff --git a/a.ts b/a.ts
index 111..222 100644
--- a/a.ts
+++ b/a.ts
@@ -1,3 +1,3 @@
-let a = 1;
+let a = 2;
 let b = 3;
 let c = 4;
@@ -20,2 +20,3 @@
 let z = 99;
+let extra = 100;
 let end = 0;
";
        let files = parse_unified(unified).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks.len(), 2);
    }
}
```

- [ ] **Step 2: Run — verify compile error**

```
cargo nextest run -p comply review::diff
```

Expected: compile error (`parse_unified`, `FileStatus`, `ReviewFile` not defined).

- [ ] **Step 3: Implement**

Above the tests:

```rust
use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub added_lines: Vec<(usize, String)>,
    pub removed_lines: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub struct ReviewFile {
    pub path: PathBuf,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
}

/// Parse a unified diff (as produced by `git diff --unified=3`) into
/// `ReviewFile`s. Binary files are skipped (no hunks available).
pub fn parse_unified(diff: &str) -> Result<Vec<ReviewFile>> {
    // Minimal state machine: detect "diff --git", "rename", "@@" hunks,
    // and track `+` / `-` prefixed lines per hunk.
    // TODO(impl): full implementation.
    let _ = diff;
    anyhow::bail!("parse_unified not implemented");
}

/// Run `git diff ...` for the given scan mode and return the parsed
/// `ReviewFile`s.
pub fn diff_for_scan(scan: &crate::cli::ScanMode, repo_root: &std::path::Path) -> Result<Vec<ReviewFile>> {
    use std::process::Command;
    let args: Vec<String> = match scan {
        // WorkingTree = unstaged changes only (matches `comply --working-tree`
        // semantics in src/changed_lines.rs). `git diff` with no ref prints
        // exactly that. DO NOT add `HEAD` — that would include staged too.
        crate::cli::ScanMode::WorkingTree => vec!["diff".into(), "--unified=3".into()],
        crate::cli::ScanMode::Staged => vec!["diff".into(), "--unified=3".into(), "--cached".into()],
        crate::cli::ScanMode::LastCommit => vec!["diff".into(), "--unified=3".into(), "HEAD~1".into(), "HEAD".into()],
        crate::cli::ScanMode::Commit(sha) => vec!["diff".into(), "--unified=3".into(), format!("{sha}~1"), sha.clone()],
        crate::cli::ScanMode::Range(from, to) => vec!["diff".into(), "--unified=3".into(), from.clone(), to.clone()],
        crate::cli::ScanMode::All(_) => anyhow::bail!("comply review requires a diff mode (--working-tree, --staged, --last-commit, --commit, --range)"),
    };
    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .context("failed to spawn git diff")?;
    if !output.status.success() {
        anyhow::bail!("git diff failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    parse_unified(std::str::from_utf8(&output.stdout).context("diff is not UTF-8")?)
}
```

Implement `parse_unified` with a minimal line-by-line state machine. See reference in `src/changed_lines.rs` for inspiration.

- [ ] **Step 4: Run tests — verify PASS**

```
cargo nextest run -p comply review::diff
```

Expected: both tests PASS.

- [ ] **Step 5: Commit**

```
git add src/review/diff.rs src/review/mod.rs
git commit -m "feat(review): add diff parser producing ReviewFile hunks"
```

### Task 1.5: Deterministic pre-filter

**Files:**
- Create: `src/review/prefilter.rs`
- Modify: `src/review/mod.rs` (`pub mod prefilter;`)

Implements the skip heuristics from decision 4.2. Each rule is a small pure function so tests are trivial.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::diff::{FileStatus, Hunk, ReviewFile};
    use std::path::PathBuf;

    fn modified(path: &str, hunks: Vec<Hunk>) -> ReviewFile {
        ReviewFile { path: PathBuf::from(path), status: FileStatus::Modified, hunks }
    }

    #[test]
    fn skips_lockfile() {
        let f = modified("bun.lock", vec![]);
        assert!(matches!(prefilter(&f), Prefilter::Skip(reason) if reason == "lockfile"));
    }

    #[test]
    fn skips_minified_js() {
        assert!(matches!(prefilter(&modified("dist/app.min.js", vec![])), Prefilter::Skip(_)));
    }

    #[test]
    fn skips_generated_marker() {
        let hunk = Hunk {
            added_lines: vec![(1, "// @generated".into()), (2, "export const x = 1;".into())],
            removed_lines: vec![],
        };
        assert!(matches!(prefilter(&modified("a.ts", vec![hunk])), Prefilter::Skip(_)));
    }

    #[test]
    fn skips_pure_rename() {
        let f = ReviewFile {
            path: PathBuf::from("b.ts"),
            status: FileStatus::Renamed { from: PathBuf::from("a.ts") },
            hunks: vec![],
        };
        assert!(matches!(prefilter(&f), Prefilter::Skip(_)));
    }

    #[test]
    fn skips_whitespace_only() {
        let hunk = Hunk {
            added_lines: vec![(1, "    ".into()), (2, "\t".into())],
            removed_lines: vec![(1, "  ".into())],
        };
        assert!(matches!(prefilter(&modified("a.ts", vec![hunk])), Prefilter::Skip(_)));
    }

    #[test]
    fn keeps_real_code_change() {
        let hunk = Hunk {
            added_lines: vec![(1, "const x = compute();".into())],
            removed_lines: vec![],
        };
        assert!(matches!(prefilter(&modified("a.ts", vec![hunk])), Prefilter::Keep));
    }

    #[test]
    fn skips_binary_extension() {
        assert!(matches!(prefilter(&modified("logo.png", vec![])), Prefilter::Skip(_)));
    }

    #[test]
    fn skips_unsupported_language() {
        // v1 supports TS/TSX/JS + Rust only.
        assert!(matches!(prefilter(&modified("page.vue", vec![])), Prefilter::Skip(_)));
        assert!(matches!(prefilter(&modified("script.py", vec![])), Prefilter::Skip(_)));
    }
}
```

- [ ] **Step 2: Run — verify compile failures**

```
cargo nextest run -p comply review::prefilter
```

Expected: compile error.

- [ ] **Step 3: Implement**

```rust
//! Deterministic pre-filter — skips files that should never hit the
//! LLM. See decision 4.2 in the decision log.

use crate::review::diff::{FileStatus, ReviewFile};

#[derive(Debug, PartialEq, Eq)]
pub enum Prefilter {
    Keep,
    Skip(&'static str),
}

const BINARY_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "svg", "pdf", "zip", "gz",
    "tar", "wasm", "woff", "woff2", "ttf", "otf", "mp3", "mp4", "mov",
];

const LOCKFILE_NAMES: &[&str] = &[
    "bun.lock", "bun.lockb", "package-lock.json", "yarn.lock",
    "pnpm-lock.yaml", "Cargo.lock", "poetry.lock", "composer.lock",
];

const SUPPORTED_EXTS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "rs"];

const GENERATED_MARKERS: &[&str] = &["@generated", "DO NOT EDIT", "Code generated by"];

pub fn prefilter(file: &ReviewFile) -> Prefilter {
    // Pure renames with no content change.
    if matches!(file.status, FileStatus::Renamed { .. }) && file.hunks.is_empty() {
        return Prefilter::Skip("pure rename");
    }

    // File-name based checks.
    if let Some(name) = file.path.file_name().and_then(|n| n.to_str()) {
        if LOCKFILE_NAMES.iter().any(|l| name.eq_ignore_ascii_case(l)) {
            return Prefilter::Skip("lockfile");
        }
        if name.ends_with(".min.js") || name.ends_with(".min.css") {
            return Prefilter::Skip("minified");
        }
        if name.ends_with(".map") {
            return Prefilter::Skip("source map");
        }
    }

    // Extension-based filters.
    let ext = file.path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if BINARY_EXTS.contains(&ext) {
        return Prefilter::Skip("binary");
    }
    if !SUPPORTED_EXTS.contains(&ext) {
        return Prefilter::Skip("unsupported language (v1: ts/tsx/js/jsx/rs)");
    }

    // Generated marker in first N added lines.
    for hunk in &file.hunks {
        for (_, line) in hunk.added_lines.iter().take(20) {
            if GENERATED_MARKERS.iter().any(|m| line.contains(m)) {
                return Prefilter::Skip("generated file marker");
            }
        }
    }

    // Whitespace-only diff.
    let meaningful = file.hunks.iter().any(|h| {
        h.added_lines.iter().chain(h.removed_lines.iter()).any(|(_, line)| !line.trim().is_empty())
    });
    if !meaningful && !file.hunks.is_empty() {
        return Prefilter::Skip("whitespace-only changes");
    }

    Prefilter::Keep
}
```

- [ ] **Step 4: Run tests — verify PASS**

```
cargo nextest run -p comply review::prefilter
```

Expected: all 8 tests PASS.

- [ ] **Step 5: Commit**

```
git add src/review/prefilter.rs src/review/mod.rs
git commit -m "feat(review): deterministic pre-filter for binaries, locks, generated, renames"
```

### Task 1.6: Add `[review]` section to `src/config/defaults.toml` + accessor + `ReviewSeverity` enum

**Files:**
- Create: `src/review/finding.rs` (enum only — Finding struct lands in M4)
- Modify: `src/config/defaults.toml`
- Modify: `src/config/mod.rs`
- Create: `src/review/config.rs`

Order matters: `finding.rs` defines `ReviewSeverity`, which `config.rs` imports. Create it first.

- [ ] **Step 1: Create `src/review/finding.rs` (M1 stub)**

```rust
//! Review-specific types. M1 ships only `ReviewSeverity`; M4 adds
//! `Finding` and `QualityScore`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReviewSeverity { Nit, Low, Medium, High, Critical }

impl ReviewSeverity {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "nit" => Self::Nit,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_valid_severity() {
        assert_eq!(ReviewSeverity::parse("medium"), Some(ReviewSeverity::Medium));
        assert_eq!(ReviewSeverity::parse("critical"), Some(ReviewSeverity::Critical));
        assert_eq!(ReviewSeverity::parse("bogus"), None);
    }

    #[test]
    fn orders_by_escalation() {
        assert!(ReviewSeverity::Nit < ReviewSeverity::Low);
        assert!(ReviewSeverity::Medium < ReviewSeverity::Critical);
    }
}
```

Register in `src/review/mod.rs`: `pub mod finding;`.

- [ ] **Step 2: Append to `src/config/defaults.toml`**

```toml
# ---------------------------------------------------------------
# comply review — LLM-driven code review (opt-in via `comply review`).
# ---------------------------------------------------------------

[review]
# Minimum severity to report. One of: critical, high, medium, low, nit.
threshold = "medium"

# Maximum findings shown in the output (top N by score desc).
max_findings = 15

# Model IDs. Triage defaults to the cheapest Haiku; findings default
# to Sonnet 4.6. `--deep` overrides findings_model to Opus.
triage_model = "claude-haiku-4-5-20251001"
findings_model = "claude-sonnet-4-6"
deep_findings_model = "claude-opus-4-7"

# Hard cap — if more than this many files survive triage, review refuses.
max_files = 50

# Per-chunk token budget (input). Files exceeding this are split by
# top-level items.
chunk_token_budget = 8000

# Cross-file context budget per chunk (signatures + caller call-sites).
cross_file_token_budget = 2000
```

- [ ] **Step 3: Add `src/review/config.rs`**

```rust
//! Typed accessor over the `[review]` section. Uses the same pattern
//! as `config::Config::threshold` — defaults.toml is the single source
//! of truth, no fallbacks in code.

use crate::config::Config;
use crate::review::finding::ReviewSeverity;

pub struct ReviewConfig<'a> {
    config: &'a Config,
}

impl<'a> ReviewConfig<'a> {
    pub fn new(config: &'a Config) -> Self { Self { config } }

    pub fn threshold(&self) -> ReviewSeverity {
        let raw = self.config.review_str("threshold");
        ReviewSeverity::parse(&raw).expect("invalid review.threshold")
    }

    pub fn max_findings(&self) -> usize {
        self.config.review_usize("max_findings")
    }

    pub fn triage_model(&self) -> String { self.config.review_str("triage_model") }
    pub fn findings_model(&self) -> String { self.config.review_str("findings_model") }
    pub fn deep_findings_model(&self) -> String { self.config.review_str("deep_findings_model") }
    pub fn max_files(&self) -> usize { self.config.review_usize("max_files") }
    pub fn chunk_token_budget(&self) -> usize { self.config.review_usize("chunk_token_budget") }
    pub fn cross_file_token_budget(&self) -> usize { self.config.review_usize("cross_file_token_budget") }
}
```

- [ ] **Step 4: Add `review_str` / `review_usize` panicking accessors in `src/config/mod.rs`**

Shape note: the existing per-rule accessors (e.g. `Config::threshold(rule_id, key)`) take two arguments because they look up per-rule subtables like `[rules.no-throw]`. The `[review]` section is **flat** — one table, one level of keys — so the new accessors take only `key`:

```rust
impl Config {
    /// Read a string from `[review]`. Panics if the key is missing; the
    /// defaults.toml is the single source of truth (see
    /// `feedback_no_threshold_fallback`).
    pub fn review_str(&self, key: &str) -> String {
        self.toml["review"][key]
            .as_str()
            .unwrap_or_else(|| panic!("missing [review].{key} in defaults.toml"))
            .to_string()
    }

    pub fn review_usize(&self, key: &str) -> usize {
        usize::try_from(
            self.toml["review"][key]
                .as_integer()
                .unwrap_or_else(|| panic!("[review].{key} is not an integer")),
        )
        .expect("[review] usize out of range")
    }
}
```

Adjust indexing syntax to match the repo's actual TOML crate (toml-edit / toml). Read one existing accessor to confirm.

- [ ] **Step 5: Tests + commit**

Add unit tests for the config accessors + ReviewSeverity::parse. Then:

```
cargo nextest run -p comply review::
cargo clippy --all --all-targets -- -D warnings
git add src/config/defaults.toml src/config/mod.rs src/review/config.rs src/review/finding.rs src/review/mod.rs
git commit -m "feat(review): config section + typed accessor + ReviewSeverity enum"
```

### Task 1.7: Wire M1 end-to-end

**Files:**
- Modify: `src/review/mod.rs`

`run(cli)` now:
1. Resolve `ScanMode` from cli
2. Call `diff::diff_for_scan` → `Vec<ReviewFile>`
3. Apply `prefilter::prefilter` to each
4. Print `KEPT` / `SKIP` lines to stderr
5. Exit 0

- [ ] **Step 1: Write integration test**

Create `tests/review_m1.rs`:

```rust
use assert_cmd::Command;
use std::fs;
use std::process::Command as StdCommand;

/// End-to-end M1: init a tmp git repo, add one committed `.ts` file,
/// modify it, run `comply review --working-tree`, assert stderr shows
/// the file as KEPT.
#[test]
fn m1_prefilter_pipeline_keeps_modified_ts_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    // Init + configure minimal identity so `git commit` works in CI.
    StdCommand::new("git").args(["init", "-q"]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "test@example.com"]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "test"]).current_dir(repo).status().unwrap();

    // Commit an initial TS file.
    let src_dir = repo.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("foo.ts");
    fs::write(&file, "export const x = 1;\n").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["commit", "-q", "-m", "init"]).current_dir(repo).status().unwrap();

    // Modify the file (unstaged change) so --working-tree picks it up.
    fs::write(&file, "export const x = 2;\nexport const y = 3;\n").unwrap();

    // Run `comply review --working-tree` from the tmp repo.
    let out = Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree"])
        .current_dir(repo)
        .output()
        .expect("spawn comply");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("KEPT src/foo.ts") || stderr.contains("KEPT src/foo.ts"),
        "expected KEPT src/foo.ts in stderr, got: {stderr}"
    );
    assert!(out.status.success(), "exit should be 0 for M1 (no LLM yet)");
}

/// Lockfiles should be filtered out deterministically.
#[test]
fn m1_prefilter_skips_lockfile() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    StdCommand::new("git").args(["init", "-q"]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["config", "user.email", "t@e.com"]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["config", "user.name", "t"]).current_dir(repo).status().unwrap();

    fs::write(repo.join("bun.lock"), "{}\n").unwrap();
    StdCommand::new("git").args(["add", "."]).current_dir(repo).status().unwrap();
    StdCommand::new("git").args(["commit", "-q", "-m", "init"]).current_dir(repo).status().unwrap();
    fs::write(repo.join("bun.lock"), "{\"a\":1}\n").unwrap();

    let out = Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree"])
        .current_dir(repo)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("SKIP bun.lock"), "expected lockfile skip, got: {stderr}");
}
```

- [ ] **Step 2: Run — verify it fails**

```
cargo nextest run --test review_m1
```

Expected: stderr doesn't contain KEPT (pipeline still stubbed).

- [ ] **Step 3: Implement the M1 pipeline in `src/review/mod.rs`**

Replace the stub `run()`:

```rust
use crate::review::prefilter::{prefilter, Prefilter};
use crate::review::diff;

pub fn run(cli: &Cli) -> Result<ExitCode> {
    let scan = cli.scan_mode();
    let repo_root = std::env::current_dir()?;
    let files = diff::diff_for_scan(&scan, &repo_root)?;
    eprintln!("comply review: {} files in diff", files.len());

    let mut kept = 0usize;
    for f in &files {
        match prefilter(f) {
            Prefilter::Keep => {
                eprintln!("KEPT {}", f.path.display());
                kept += 1;
            }
            Prefilter::Skip(reason) => {
                eprintln!("SKIP {}  ({})", f.path.display(), reason);
            }
        }
    }
    eprintln!("comply review: {kept} files survived pre-filter");
    Ok(ExitCode::from(0))
}
```

- [ ] **Step 4: Run test — verify PASS**

```
cargo nextest run --test review_m1
cargo clippy --all --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```
git add src/review/mod.rs tests/review_m1.rs
git commit -m "feat(review): M1 end-to-end — diff parse + pre-filter pipeline"
```

### M1 exit criteria

- `comply review --last-commit` runs without network calls
- Pre-filter drops known-bad paths with a reason line
- `cargo nextest run` passes
- `cargo clippy --all --all-targets -- -D warnings` passes

---

## M2 — Haiku triage + skill selection

**Goal:** For each NEEDS_REVIEW candidate post-pre-filter, call Haiku via the worker with the diff + metadata. Haiku returns `{verdict, reason, skills[]}`. Skills are drawn from the whitelist. Output at this stage: NDJSON on stderr `{path, verdict, skills[]}`.

This milestone introduces the Bun worker, backend abstraction, and skill loader.

### Task 2.1: Skill loader — list + whitelist + frontmatter

**Files:**
- Create: `src/review/skill.rs`

`load_skills()` walks `$HOME/.claude/skills/<name>/SKILL.md`, parses YAML frontmatter, returns `Vec<Skill>` filtered by whitelist.

- [ ] **Step 1: Define the whitelist inline**

```rust
/// Skills eligible for review injection. Declared here rather than in
/// each SKILL.md so unrelated skills (e.g. `grill-me`, `paperasse`) are
/// never accidentally injected.
pub const WHITELIST: &[&str] = &[
    "react", "frontend", "vue", "language-rust", "language-typescript",
    "language-swift", "drizzle-orm", "better-auth-best-practices", "i18n",
    "tailwind", "coding-standards", "security-defensive", "api-design",
    "database", "testing", "tanstack-query", "zod",
    "tanstack-start-best-practices", "better-result-adopt", "tdd",
    "react-native", "shadcn", "docker", "ci-cd", "kubernetes",
    "web-performance", "ui-animations",
];
```

- [ ] **Step 2: Tests first**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter_name_and_description() {
        let body = "---\nname: react\ndescription: React perf\n---\n\n# body";
        let skill = parse(body, "react").unwrap();
        assert_eq!(skill.id, "react");
        assert_eq!(skill.description, "React perf");
        assert!(skill.body.contains("# body"));
    }

    #[test]
    fn rejects_skill_without_frontmatter() {
        assert!(parse("no frontmatter here", "x").is_err());
    }

    #[test]
    fn whitelist_filters_unlisted_skills() {
        assert!(WHITELIST.contains(&"react"));
        assert!(!WHITELIST.contains(&"grill-me"));
        assert!(!WHITELIST.contains(&"paperasse:notaire"));
    }
}
```

- [ ] **Step 3: Implement**

```rust
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct Skill {
    pub id: String,
    pub description: String,
    pub body: String,
    /// SHA256 of `body`. Goes into the review cache key so a skill
    /// edit invalidates the cache (decision 8.1).
    pub body_hash: String,
}

pub fn load_skills() -> Result<Vec<Skill>> {
    let home = std::env::var("HOME").context("HOME not set")?;
    let skills_dir = PathBuf::from(home).join(".claude/skills");
    if !skills_dir.exists() { return Ok(vec![]); }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let id = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !WHITELIST.contains(&id.as_str()) { continue; }
        let skill_md = entry.path().join("SKILL.md");
        if !skill_md.exists() { continue; }
        let body = std::fs::read_to_string(&skill_md)?;
        if let Ok(skill) = parse(&body, &id) {
            out.push(skill);
        }
    }
    Ok(out)
}

fn parse(md: &str, id: &str) -> Result<Skill> {
    let rest = md.strip_prefix("---\n").context("missing frontmatter")?;
    let (fm, body) = rest.split_once("\n---\n").context("unterminated frontmatter")?;
    let description = fm
        .lines()
        .find_map(|l| l.strip_prefix("description:").map(|s| s.trim().to_string()))
        .unwrap_or_default();
    use sha2::{Digest, Sha256};
    let body_hash = hex::encode(Sha256::digest(body.as_bytes()));
    Ok(Skill { id: id.into(), description, body: body.to_string(), body_hash })
}
```

Add `sha2` and `hex` to `Cargo.toml` if missing.

- [ ] **Step 4: Verify + commit**

```
cargo nextest run -p comply review::skill
git add src/review/skill.rs src/review/mod.rs Cargo.toml
git commit -m "feat(review): skill loader with whitelist + body hash"
```

### Task 2.2: Backend abstraction

**Files:**
- Create: `src/review/backend.rs`

```rust
//! Backend dispatch — α (claude-code) by default, β (anthropic direct)
//! when COMPLY_REVIEW_BACKEND=anthropic is set. The worker (TS) reads
//! the same env var and routes accordingly. See decision Q_CACHE.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend { ClaudeCode, Anthropic }

impl Backend {
    pub fn from_env() -> Self {
        match std::env::var("COMPLY_REVIEW_BACKEND").as_deref() {
            Ok("anthropic") => Self::Anthropic,
            _ => Self::ClaudeCode,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Anthropic => "anthropic",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_claude_code_without_env() {
        // SAFETY: serial_test not available here. Rely on the fact that
        // we never set the env var in CI default; worst case: this test
        // passes spuriously if something else sets it. Document it.
        temp_env::with_var_unset("COMPLY_REVIEW_BACKEND", || {
            assert_eq!(Backend::from_env(), Backend::ClaudeCode);
        });
    }

    #[test]
    fn switches_when_env_is_anthropic() {
        temp_env::with_var("COMPLY_REVIEW_BACKEND", Some("anthropic"), || {
            assert_eq!(Backend::from_env(), Backend::Anthropic);
        });
    }
}
```

Add `temp-env` dev-dep. Test + commit.

### Task 2.3: Review worker (TS) — stub with triage mode

**Files:**
- Create: `tools/review-worker.ts`
- Modify: `tools/package.json` (add `@ai-sdk/anthropic` as `optionalDependencies`)

Protocol (unchanged from `llm-worker.ts` except request schema):

- stdin: JSON array of `{ id, mode: "triage" | "findings", prompt, model, backend }`
- stdout: NDJSON `{ id, result }` or `{ id, error }` per job
- stderr: progress

Implementation:

```ts
// tools/review-worker.ts
import { generateObject } from 'ai'
import { createClaudeCode } from 'ai-sdk-provider-claude-code'
import pMap from 'p-map'
import { z } from 'zod'

const TriageSchema = z.object({
  verdict: z.enum(['NEEDS_REVIEW', 'APPROVED']),
  reason: z.string(),
  skills: z.array(z.string()),
})

const FindingSchema = z.object({
  line: z.number(),
  severity: z.enum(['critical', 'high', 'medium', 'low', 'nit']),
  category: z.string(),
  message: z.string(),
  suggestion: z.string().optional(),
})

const FindingsSchema = z.object({
  findings: z.array(FindingSchema),
  quality_score: z.number().min(0).max(10),
  file_summary: z.string(),
})

type Mode = 'triage' | 'findings'
type BackendId = 'claude-code' | 'anthropic'
type Job = { id: string; mode: Mode; prompt: string; model: string; backend: BackendId }

// α backend — claude-code provider (Max sub, no API key).
const claudeCode = createClaudeCode()

// β backend — lazy loaded so α users without @ai-sdk/anthropic installed
// don't crash at startup.
async function anthropicModel(model: string) {
  const { anthropic } = await import('@ai-sdk/anthropic')
  return anthropic(model)
}

async function callModel(job: Job) {
  const schema = job.mode === 'triage' ? TriageSchema : FindingsSchema
  if (job.backend === 'anthropic') {
    const model = await anthropicModel(job.model)
    return generateObject({
      model,
      schema,
      prompt: job.prompt,
      providerOptions: {
        // Explicit cache breakpoint on the stable prefix (system +
        // review_guidelines + skills). The prompt builder on the Rust
        // side MUST place the breakpoint marker at the boundary.
        anthropic: {} as any, // placeholder: real impl in M6
      },
    })
  }
  return generateObject({
    model: claudeCode(job.model, {
      permissionMode: 'bypassPermissions',
      allowedTools: [],
      maxTurns: 1,
    }),
    schema,
    prompt: job.prompt,
  })
}

// retry / error handling identical to llm-worker.ts — copy-paste then
// adapt. See M6 Task 6.4 for the full retry policy.

async function main() {
  const input = await Bun.stdin.text()
  const jobs: Job[] = JSON.parse(input)
  if (jobs.length === 0) process.exit(0)
  await pMap(jobs, async (job) => {
    try {
      const { object } = await callModel(job)
      process.stdout.write(JSON.stringify({ id: job.id, result: JSON.stringify(object) }) + '\n')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      process.stdout.write(JSON.stringify({ id: job.id, error: msg.slice(0, 500) }) + '\n')
    }
  }, { concurrency: 5 })
}
main().catch((err) => { process.stderr.write(`review-worker fatal: ${err}\n`); process.exit(1) })
```

- [ ] **Step 1: Create the file with the content above (no caching yet, stub only)**
- [ ] **Step 2: `cd tools && bun install` to resolve the new dep**
- [ ] **Step 3: Smoke test with a canned triage job**

```
echo '[{"id":"test","mode":"triage","prompt":"Is the text SAFE? Answer JSON {verdict,reason,skills}.","model":"claude-haiku-4-5-20251001","backend":"claude-code"}]' \
  | bun run tools/review-worker.ts
```

Expect one JSON line with `{id:"test", result: "..."}` or an error — at this point success depends on your Claude session.

- [ ] **Step 4: Commit**

```
git add tools/review-worker.ts tools/package.json tools/bun.lock
git commit -m "feat(review): bun worker with triage/findings modes and α/β backends"
```

### Task 2.4: Triage caller in Rust

**Files:**
- Create: `src/review/worker.rs` (spawn + NDJSON round-trip, adapted from `src/llm/mod.rs::invoke_worker`)
- Create: `src/review/triage.rs` (build triage prompts, parse `Triage` response)

`triage.rs`:

```rust
use crate::review::diff::ReviewFile;
use crate::review::skill::Skill;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum Verdict { NeedsReview, Approved }

#[derive(Debug)]
pub struct Triage {
    pub path: std::path::PathBuf,
    pub verdict: Verdict,
    pub reason: String,
    /// Skill IDs picked by Haiku, filtered to the whitelist.
    pub skills: Vec<String>,
}

pub fn build_prompt(file: &ReviewFile, available_skills: &[Skill]) -> String {
    // XML-ish prompt. Includes: diff hunks + metadata + available skill
    // IDs/descriptions. Haiku must answer JSON { verdict, reason, skills[] }.
    // See docs/plans/…-decision-log.md decisions 4.1, 6.1, 6.6.
    let skills_xml = available_skills
        .iter()
        .map(|s| format!("  <skill id=\"{}\">{}</skill>", s.id, s.description))
        .collect::<Vec<_>>()
        .join("\n");
    let hunks = file
        .hunks
        .iter()
        .map(|h| {
            let added = h.added_lines.iter().map(|(n, l)| format!("+ L{n}: {l}")).collect::<Vec<_>>().join("\n");
            let removed = h.removed_lines.iter().map(|(n, l)| format!("- L{n}: {l}")).collect::<Vec<_>>().join("\n");
            format!("{removed}\n{added}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "You are triaging a file in a code review. Decide if it NEEDS_REVIEW or APPROVED, and pick which skill guidelines apply.\n\
         Return JSON {{verdict, reason, skills[]}} where skills is an array of ids drawn from the <available_skills> list below.\n\n\
         <file path=\"{path}\" status=\"{status:?}\" hunks_count=\"{n}\">\n{hunks}\n</file>\n\n\
         <available_skills>\n{skills_xml}\n</available_skills>\n",
        path = file.path.display(),
        status = file.status,
        n = file.hunks.len(),
    )
}

#[derive(Deserialize)]
struct TriageRaw { verdict: String, reason: String, skills: Vec<String> }

pub fn parse_response(json: &str, path: &std::path::Path, allowed_skill_ids: &[String]) -> anyhow::Result<Triage> {
    let raw: TriageRaw = serde_json::from_str(json)?;
    let verdict = match raw.verdict.as_str() {
        "NEEDS_REVIEW" => Verdict::NeedsReview,
        "APPROVED" => Verdict::Approved,
        _ => anyhow::bail!("invalid verdict: {}", raw.verdict),
    };
    let skills = raw.skills.into_iter().filter(|s| allowed_skill_ids.iter().any(|a| a == s)).collect();
    Ok(Triage { path: path.to_path_buf(), verdict, reason: raw.reason, skills })
}
```

Inline tests cover prompt formatting and parse filtering.

- [ ] **Step 1-4 (standard TDD cycle)**
- [ ] **Step 5: Commit**

```
git commit -m "feat(review): Haiku triage — prompt builder, response parser, skill filtering"
```

### Task 2.5: Wire triage into M1 pipeline

**Files:**
- Modify: `src/review/mod.rs`

Pipeline becomes:

```rust
let files = diff::diff_for_scan(&scan, &repo_root)?;
let kept: Vec<&ReviewFile> = files.iter().filter(|f| matches!(prefilter(f), Prefilter::Keep)).collect();
let skills = skill::load_skills()?;
let backend = backend::Backend::from_env();

let triage_jobs: Vec<_> = kept.iter().map(|f| WorkerJob {
    id: f.path.display().to_string(),
    mode: "triage",
    prompt: triage::build_prompt(f, &skills),
    model: config.review().triage_model(),
    backend: backend.as_str(),
}).collect();
let results = worker::invoke(&triage_jobs)?;
// parse, emit NDJSON `{path, verdict, skills}` on stderr
```

- [ ] Full TDD cycle with a `COMPLY_REVIEW_WORKER_STUB` env var that short-circuits `worker::invoke` to a canned response.

### M2 exit criteria

- `COMPLY_REVIEW_WORKER_STUB=1 comply review --last-commit` prints a triage result per KEPT file
- `COMPLY_REVIEW_BACKEND=anthropic` switches the worker to the β path (end-to-end smoke test optional)
- All inline tests + integration test pass
- Clippy clean

---

## M3 — 1-hop dep graph + tree-sitter hunk widening

**Goal:** For each NEEDS_REVIEW file, (a) widen each hunk to the enclosing tree-sitter top-level item, (b) compute 1-hop neighbors (importers + imported files), (c) build ≤ 2k-token cross-file context.

### Task 3.1: `widen.rs` — tree-sitter hunk → enclosing item

**Files:**
- Create: `src/review/widen.rs`
- Modify: `src/review/mod.rs` (`pub mod widen;`)

Same pattern as the existing `src/rules/walker.rs`. For each added line number, find the smallest AST node with kind ∈ `{function_declaration, function_expression, arrow_function, method_definition, class_declaration, impl_item, function_item}` containing that line. Produce `WidenedChunk { path, source, ranges: Vec<(start_byte, end_byte)> }`.

- [ ] **Step 1: Write failing tests (one per language/shape)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_hunk_widens_to_enclosing_function() {
        let src = "function greet(name: string) {\n  const msg = 'hi ' + name;\n  return msg;\n}\n";
        let chunk = widen_ts(src, &[2]).unwrap();  // line 2 (const msg ...)
        assert!(chunk.source.contains("function greet"));
        assert!(chunk.source.contains("return msg;"));
    }

    #[test]
    fn tsx_hunk_widens_to_component() {
        let src = "export function Button() {\n  return <div>hi</div>;\n}\n";
        let chunk = widen_tsx(src, &[2]).unwrap();
        assert!(chunk.source.contains("export function Button"));
    }

    #[test]
    fn rust_hunk_widens_to_impl_method() {
        let src = "impl Foo {\n  fn bar(&self) -> u32 {\n    42\n  }\n}\n";
        let chunk = widen_rust(src, &[3]).unwrap();
        assert!(chunk.source.contains("fn bar"));
    }

    #[test]
    fn rust_hunk_widens_to_free_function() {
        let src = "fn top() {\n  println!(\"hi\");\n}\n";
        let chunk = widen_rust(src, &[2]).unwrap();
        assert!(chunk.source.contains("fn top"));
    }

    #[test]
    fn falls_back_to_full_file_when_widening_fails() {
        // Macro-wrapped code — tree-sitter can't descend into macro bodies.
        let src = "macro_rules! x { () => {} }\nfn main(){}\n";
        let chunk = widen_rust(src, &[1]).unwrap();
        assert_eq!(chunk.source, src, "fallback returns full source");
    }
}
```

- [ ] **Step 2: Run — verify compile error**

```
cargo nextest run -p comply review::widen
```

- [ ] **Step 3: Implement**

Public API:

```rust
pub struct WidenedChunk {
    pub path: std::path::PathBuf,
    pub source: String,
    pub ranges: Vec<(usize, usize)>, // (start_byte, end_byte) per widened span
}

pub fn widen_ts(source: &str, lines: &[usize]) -> anyhow::Result<WidenedChunk>;
pub fn widen_tsx(source: &str, lines: &[usize]) -> anyhow::Result<WidenedChunk>;
pub fn widen_rust(source: &str, lines: &[usize]) -> anyhow::Result<WidenedChunk>;
```

For each target line, walk the tree-sitter AST via `tree_sitter::TreeCursor` to find the deepest ancestor whose `kind()` is in the enclosing-item set. Merge overlapping ranges. If any call panics or returns no match, fall back to the entire source as a single range.

- [ ] **Step 4: Verify tests PASS**

```
cargo nextest run -p comply review::widen
cargo clippy --all --all-targets -- -D warnings
```

- [ ] **Step 5: Commit**

```
git add src/review/widen.rs src/review/mod.rs
git commit -m "feat(review): tree-sitter hunk widening to enclosing function/impl/class"
```

### Task 3.2: TS dep resolver via `oxc_resolver`

**Files:**
- Modify: `Cargo.toml` (add `oxc_resolver` as dep — already used for oxlint integration? verify; if not, add it)
- Create: `src/review/dep_graph.rs` with `ts_resolver` module

API:

```rust
pub struct DepGraph {
    // 1-hop edges only. Key: file path, value: paths it imports + paths
    // that import it (ingoing and outgoing edges stored together).
    pub neighbors: HashMap<PathBuf, Neighbors>,
}

pub struct Neighbors {
    pub imports: Vec<PathBuf>,
    pub importers: Vec<PathBuf>,
}

/// Build a partial graph: only the `files` given and their 1-hop
/// neighbors. Walk candidate import roots (configured `src/`) to find
/// importers. Use oxc_resolver for TS/JS, ad-hoc mod parser for Rust.
pub fn build_partial(files: &[PathBuf], repo_root: &Path) -> Result<DepGraph>;
```

Fixtures under `tests/fixtures/dep_graph/ts/`:
- `tests/fixtures/dep_graph/ts/src/a.ts` — exports `foo`
- `tests/fixtures/dep_graph/ts/src/b.ts` — `import { foo } from './a'`
- `tests/fixtures/dep_graph/ts/tsconfig.json` — minimal config for `oxc_resolver`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn ts_partial_graph_finds_direct_importer() {
    let root = std::path::Path::new("tests/fixtures/dep_graph/ts");
    let g = build_partial(&[root.join("src/a.ts")], root).unwrap();
    let n = g.neighbors.get(&root.join("src/a.ts")).unwrap();
    assert!(n.importers.iter().any(|p| p.ends_with("b.ts")));
}
```

- [ ] **Step 2: Create fixtures + run test → expect fail**
- [ ] **Step 3: Implement `build_partial` walking `<repo>/src/**/*.{ts,tsx,js,jsx,mjs,cjs}` and resolving via `oxc_resolver::Resolver::new(...).resolve(base, spec)`**
- [ ] **Step 4: Verify tests PASS + clippy clean**
- [ ] **Step 5: Commit**

```
git add src/review/dep_graph.rs tests/fixtures/dep_graph/ts/
git commit -m "feat(review): partial 1-hop TS dep graph via oxc_resolver"
```

### Task 3.3: Rust dep resolver — manual mod tree

**Files:**
- Modify: `src/review/dep_graph.rs` (add `rust_resolver` submodule)

Rust has no external resolver crate. Parse `use` statements via tree-sitter Rust, walk `mod x;` declarations from `lib.rs`/`main.rs`, resolve to files.

Edge cases acknowledged (acceptable to punt with TODO comments): `mod tests { ... }`, `#[path = "..."]` attribute, workspace members, `pub use` re-exports. At v1 we cover the 80 % case: regular `mod foo;` + `foo/mod.rs` or `foo.rs`.

Fixtures under `tests/fixtures/dep_graph/rust/`:
- `src/lib.rs` with `pub mod alpha;`
- `src/alpha.rs` with `pub fn hello() {}`
- `src/beta.rs` with `use crate::alpha::hello;`
- `src/lib.rs` also declares `pub mod beta;`

- [ ] **Step 1: Write failing test — `alpha.rs` has `beta.rs` as importer**
- [ ] **Step 2: Run — compile fail**
- [ ] **Step 3: Implement mod-tree walker: from `src/lib.rs` or `src/main.rs`, recursively parse `mod foo;`; for each found module, parse its `use crate::…` items and link back**
- [ ] **Step 4: Verify tests PASS**
- [ ] **Step 5: Commit**

```
git add src/review/dep_graph.rs tests/fixtures/dep_graph/rust/
git commit -m "feat(review): partial Rust dep graph via mod-tree walker"
```

### Task 3.4: Cross-file context builder

**Files:**
- Create: `src/review/cross_file.rs`
- Modify: `src/review/mod.rs` (`pub mod cross_file;`)

For target file T and its neighbors N:

1. For each `imported` file in N.imports: extract exported signatures (fn/type/const) via tree-sitter, no bodies, add as context.
2. For each `importer` file in N.importers: find each line that calls a symbol T exports, include that line ± 2 lines for context.
3. Enforce token budget (config `cross_file_token_budget`, default 2000). Tokens estimated as `bytes / 4` (heuristic consistent with OpenAI's general tokenizer ratio; drift acceptable because we only need "roughly").
4. Priority: callers touched by the diff first, then by alphabetical stability.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn signatures_extracted_without_bodies() {
    let src = "export function foo(x: number): string { return String(x); }";
    let sigs = extract_exported_signatures_ts(src).unwrap();
    assert_eq!(sigs.len(), 1);
    assert!(sigs[0].contains("foo(x: number): string"));
    assert!(!sigs[0].contains("return"));
}

#[test]
fn prioritises_callers_touched_by_diff() {
    let context = build_cross_file_context(
        /* target */ "src/t.ts",
        /* importers */ &["src/a.ts".into(), "src/b.ts".into(), "src/c.ts".into()],
        /* diff_touched */ &["src/b.ts".into(), "src/c.ts".into()],
        /* budget */ 500,
        /* resolve_source */ |p| Some(format!("// stub for {}", p.display())),
    );
    // b and c come before a in the output.
    let idx_b = context.find("b.ts").unwrap();
    let idx_c = context.find("c.ts").unwrap();
    let idx_a = context.find("a.ts");
    if let Some(a) = idx_a {
        assert!(idx_b < a && idx_c < a);
    }
}

#[test]
fn respects_token_budget_bytes_over_four() {
    // Budget 100 tokens ≈ 400 bytes of content.
    let huge = "x".repeat(2_000);
    let out = build_cross_file_context(
        "t.ts",
        &["a.ts".into()],
        &[],
        100,
        |_| Some(huge.clone()),
    );
    assert!(out.len() <= 500, "should truncate to ~budget");
}
```

- [ ] **Step 2: Run — expect compile fail**
- [ ] **Step 3: Implement**
- [ ] **Step 4: Verify PASS + clippy clean**
- [ ] **Step 5: Commit**

```
git add src/review/cross_file.rs src/review/mod.rs
git commit -m "feat(review): cross-file context builder with diff-priority + token budget"
```

### M3 exit criteria

- Widening produces function-scoped chunks for TS and Rust
- `dep_graph::build_partial` runs in < 500ms on a 200-file TS project
- Cross-file context respects budget, prioritizes diff-touched callers
- Inline tests + fixture tests pass

---

## M4 — Sonnet findings + scoring + blast radius + dedup

### Task 4.1: `finding.rs` full

Extend the M1 stub:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReviewSeverity { Nit, Low, Medium, High, Critical }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub severity: ReviewSeverity,
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
    /// Pre-boost severity (what the LLM returned).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_severity: Option<ReviewSeverity>,
}

#[derive(Debug, Clone)]
pub struct QualityScore(pub u8); // 0..=10

impl ReviewSeverity {
    /// Blast-radius boost from decision 5.2:
    /// `severity_boost = min(1, log2(importers+1) / 3)`.
    ///
    /// The formula produces a continuous value in `[0, 1]` (saturates at
    /// 8 importers). Severity is categorical with 5 levels, so we quantize
    /// at the 0.5 midpoint: boost ≥ 0.5 → bump one level, else stay.
    /// `log2(x+1)/3 ≥ 0.5` ⇔ `x ≥ 2^1.5 - 1 ≈ 1.83`, i.e. **≥ 2 importers
    /// bumps one level**, saturating at Critical.
    pub fn boost(self, importers: usize) -> Self {
        let raw_boost = ((importers as f64 + 1.0).log2() / 3.0).min(1.0);
        let bump = raw_boost >= 0.5;
        if !bump {
            return self;
        }
        match self {
            Self::Nit => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Critical,
            Self::Critical => Self::Critical,
        }
    }
}
```

Tests for parse + boost:

```rust
#[test]
fn boost_is_identity_with_zero_importers() {
    assert_eq!(ReviewSeverity::Medium.boost(0), ReviewSeverity::Medium);
}

#[test]
fn boost_bumps_at_two_importers() {
    // log2(3)/3 ≈ 0.528 → bumps
    assert_eq!(ReviewSeverity::Low.boost(2), ReviewSeverity::Medium);
    assert_eq!(ReviewSeverity::Medium.boost(5), ReviewSeverity::High);
}

#[test]
fn boost_does_not_bump_with_single_importer() {
    // log2(2)/3 ≈ 0.333 → no bump
    assert_eq!(ReviewSeverity::Low.boost(1), ReviewSeverity::Low);
}

#[test]
fn boost_saturates_at_critical() {
    assert_eq!(ReviewSeverity::Critical.boost(100), ReviewSeverity::Critical);
    assert_eq!(ReviewSeverity::High.boost(100), ReviewSeverity::Critical);
}
```

### Task 4.2: Findings prompt builder

**Files:**
- Create: `src/review/prompt.rs`

Prompt structure (decision 6.5):

```
<review_guidelines>
  <guideline source="react">{FULL_SKILL_BODY}</guideline>
  <guideline source="drizzle-orm">{FULL_SKILL_BODY}</guideline>
</review_guidelines>

<file_under_review path="src/foo.ts">
{WIDENED_CODE}
</file_under_review>

<cross_file_context>
{SIGNATURES_AND_CALLSITES}
</cross_file_context>

Produce JSON matching this schema:
{
  findings: [{line, severity, category, message, suggestion?}],
  quality_score: 0..10,
  file_summary: string
}
Rules:
- Only report issues on <file_under_review>, not on context.
- Severity: critical|high|medium|low|nit. critical = security/data-loss; nit = cosmetic.
- One finding per issue; do not duplicate.
```

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn prompt_contains_all_required_blocks() {
    let skills = vec![Skill { id: "react".into(), description: "React".into(), body: "# React rules".into(), body_hash: "h".into() }];
    let file_src = "function Foo() { return <div />; }";
    let cross = "// no cross context";
    let prompt = build_findings_prompt("src/Foo.tsx", file_src, cross, &skills);

    assert!(prompt.contains("<review_guidelines>"));
    assert!(prompt.contains("<guideline source=\"react\">"));
    assert!(prompt.contains("# React rules"));
    assert!(prompt.contains("<file_under_review path=\"src/Foo.tsx\">"));
    assert!(prompt.contains(file_src));
    assert!(prompt.contains("<cross_file_context>"));
    assert!(prompt.contains("findings: [{line, severity, category"));
}

#[test]
fn prompt_omits_guidelines_block_when_no_skills() {
    let prompt = build_findings_prompt("src/x.ts", "x", "", &[]);
    assert!(!prompt.contains("<review_guidelines>"));
}
```

- [ ] **Step 2: Run — compile fail**
- [ ] **Step 3: Implement `build_findings_prompt`**
- [ ] **Step 4: Verify PASS + clippy**
- [ ] **Step 5: Commit**

```
git add src/review/prompt.rs src/review/mod.rs
git commit -m "feat(review): findings prompt builder with XML-ish skill/file/context blocks"
```

### Task 4.3: Chunk builder

**Files:**
- Create: `src/review/chunk.rs`
- Modify: `src/review/mod.rs` (`pub mod chunk;`)

Combines widened code + cross-file context + skill bodies. Enforces `chunk_token_budget` (default 8000 tokens estimated). If a file exceeds, split by top-level tree-sitter items; each sub-chunk shares the full skills + cross-file context (acceptable, these fit in the 200k window easily).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn single_chunk_when_under_budget() {
    let widened = WidenedChunk { path: "a.ts".into(), source: "x".repeat(100), ranges: vec![(0, 100)] };
    let chunks = build_chunks(&widened, "cross", &[], 8_000);
    assert_eq!(chunks.len(), 1);
}

#[test]
fn splits_by_top_level_items_when_over_budget() {
    // Create a fake widened source that is 40_000 bytes ≈ 10k tokens,
    // containing 4 top-level tree-sitter items.
    let big = (0..4).map(|i| format!("function f{i}() {{\n{}\n}}\n", "x".repeat(9_900))).collect::<String>();
    let widened = WidenedChunk { path: "a.ts".into(), source: big, ranges: vec![] };
    let chunks = build_chunks(&widened, "cross", &[], 8_000);
    assert!(chunks.len() >= 2, "should split into ≥2 chunks");
}
```

- [ ] **Step 2: Run — compile fail**
- [ ] **Step 3: Implement `build_chunks` with tree-sitter item boundary detection**
- [ ] **Step 4: Verify PASS + clippy**
- [ ] **Step 5: Commit**

```
git add src/review/chunk.rs src/review/mod.rs
git commit -m "feat(review): chunk builder with top-level item split past token budget"
```

### Task 4.4: Scoring — blast radius + dedup + threshold + cap

**Files:**
- Create: `src/review/scoring.rs`

```rust
pub fn score_and_filter(
    findings: Vec<Finding>,
    dep_graph: &DepGraph,
    threshold: ReviewSeverity,
    max_findings: usize,
) -> Vec<Finding> {
    let boosted = findings.into_iter().map(|mut f| {
        let importers = dep_graph.neighbors.get(&f.path).map(|n| n.importers.len()).unwrap_or(0);
        f.raw_severity = Some(f.severity);
        f.severity = f.severity.boost(importers);
        f
    });
    let deduped = dedup_exact(boosted.collect());
    let filtered: Vec<_> = deduped.into_iter().filter(|f| f.severity >= threshold).collect();
    let mut sorted = filtered;
    sorted.sort_by(|a, b| b.severity.cmp(&a.severity));
    sorted.truncate(max_findings);
    sorted
}

fn dedup_exact(mut findings: Vec<Finding>) -> Vec<Finding> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    findings.retain(|f| {
        let key = (f.path.clone(), f.line, hash_message(&f.message));
        seen.insert(key)
    });
    findings
}

fn hash_message(msg: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    msg.hash(&mut h);
    h.finish()
}
```

Tests: (1) single finding, no importers → unchanged. (2) 5 importers → severity boost up one level. (3) duplicates on same path+line+message → deduped. (4) threshold filters correctly. (5) cap to 15.

### Task 4.5: Wire findings into pipeline

**Files:**
- Modify: `src/review/mod.rs`

Pipeline now:

1. diff + prefilter (M1)
2. triage per file (M2) → filter to NEEDS_REVIEW
3. hard cap 50 files (abort with `ReviewError::DiffTooLarge` if exceeded — M6 covers, stub for now)
4. build dep graph (M3)
5. widen hunks (M3) + build cross-file context (M3)
6. build chunks (M4)
7. parallel Sonnet findings calls, concurrency 5 (M4)
8. parse findings, score + dedup + threshold + cap (M4)
9. print to stderr as JSON (M5 adds pretty output)

- [ ] **Step 1: Write integration test `tests/review_m4.rs`**

Uses `COMPLY_REVIEW_WORKER_STUB=findings:<fixture>.json` so the worker returns canned findings. Asserts the final JSON output contains exactly the findings that pass threshold, dedup, and cap.

- [ ] **Step 2: Run — expect fail (pipeline not yet calling scoring)**
- [ ] **Step 3: Wire stages 4-8 in `review::run`**
- [ ] **Step 4: Verify test PASS + clippy**
- [ ] **Step 5: Commit**

```
git add src/review/mod.rs tests/review_m4.rs tests/fixtures/review/
git commit -m "feat(review): M4 end-to-end — findings + scoring + dedup + threshold"
```

### M4 exit criteria

- End-to-end run produces `Vec<Finding>` respecting threshold, dedup, cap
- Blast radius boosts severity for highly-imported files
- All tests pass, clippy clean

---

## M5 — Markdown & JSON output + exit codes

### Task 5.1: `output/markdown.rs`

Format:

```
# comply review

## TL;DR
<GLOBAL_SUMMARY — from M4/M5.3 Sonnet summary call>

- Files reviewed: N
- Findings: X (severity: C critical, H high, M medium, L low)
- Average quality score: 7.2 / 10

## Findings

### src/foo.ts (quality: 6/10)
**File summary:** <from Sonnet>

- **[HIGH] L42 — missing Suspense boundary around useSearchParams()**
  Use `<Suspense>` to avoid forcing CSR. Suggestion: wrap `<Results />`.
  _(raw severity: medium, boosted by 8 importers)_

- **[MEDIUM] L87 — …**
```

Unit tests on a canned `Vec<Finding>` + expected markdown output (use `insta` snapshots if already in dev-deps, else string asserts).

### Task 5.2: `output/json.rs`

NDJSON: one header line `{type:"summary", ...}` then one line per finding. Machine-friendly; easy to pipe into `jq`.

### Task 5.3: Exit codes

- 0 if zero findings pass threshold
- 1 if ≥ 1 finding passes threshold
- 2 if a `ReviewError` other than "findings present" bubbles up

**Files:**
- Modify: `src/review/mod.rs` (compute exit code from findings count)

**Pre-req:** Task 1.2 already widened `main::run` to `Result<ExitCode>`, so this task only needs to compute the right `ExitCode` inside `review::run`.

- [ ] **Step 1: Write failing integration tests**

```rust
// tests/review_exit_codes.rs
use assert_cmd::Command;

#[test]
fn exits_0_when_no_findings() {
    // Stubbed worker returns empty findings.
    Command::cargo_bin("comply").unwrap()
        .args(["review", "--last-commit"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:empty")
        .assert()
        .code(0);
}

#[test]
fn exits_1_when_findings_above_threshold() {
    Command::cargo_bin("comply").unwrap()
        .args(["review", "--last-commit"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:one-high")
        .assert()
        .code(1);
}

#[test]
fn exits_2_on_diff_too_large() {
    Command::cargo_bin("comply").unwrap()
        .args(["review", "--last-commit"])
        .env("COMPLY_REVIEW_WORKER_STUB", "triage:too-many-files")
        .assert()
        .code(2);
}
```

- [ ] **Step 2: Run — expect all three to fail**
- [ ] **Step 3: Implement exit-code computation**

At the end of `review::run(...)`:

```rust
let exit = if findings.is_empty() {
    ExitCode::from(0)
} else {
    ExitCode::from(1)
};
Ok(exit)
```

`ReviewError::DiffTooLarge` already propagates via `?`; `main`'s existing `Err` arm maps to ExitCode::from(2).

- [ ] **Step 4: Verify tests PASS + clippy clean**
- [ ] **Step 5: Commit**

```
git add src/review/mod.rs tests/review_exit_codes.rs
git commit -m "feat(review): exit 0/1/2 per finding count + error class"
```

### Task 5.4: Global summary (Sonnet call)

**Files:**
- Create: `src/review/summary.rs`
- Modify: `src/review/mod.rs` (`pub mod summary;` + call site)

After findings are collected, make a single Sonnet call:

- Input: list of per-file summaries from the findings calls + global stats
- Output: `{ tl_dr: string, quality_overall: number }`
- Map-reduce path: if > 15 files, split per-file summaries into 3 groups, summarize each, then summarize the summaries.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn single_pass_when_under_threshold() {
    let summaries: Vec<FileSummary> = (0..10).map(|i| FileSummary {
        path: format!("f{i}.ts").into(), quality: 7, summary: format!("file {i} ok")
    }).collect();
    assert_eq!(plan_summary(&summaries).mode, SummaryMode::SinglePass);
}

#[test]
fn map_reduce_when_over_threshold() {
    let summaries: Vec<FileSummary> = (0..30).map(|i| FileSummary {
        path: format!("f{i}.ts").into(), quality: 7, summary: format!("file {i}")
    }).collect();
    let plan = plan_summary(&summaries);
    assert_eq!(plan.mode, SummaryMode::MapReduce);
    assert_eq!(plan.groups.len(), 3);
    // Total files across groups must equal input.
    let total: usize = plan.groups.iter().map(|g| g.len()).sum();
    assert_eq!(total, 30);
}

#[test]
fn global_quality_is_average_of_file_qualities() {
    // Unit test on the cheap post-processing, not the LLM call itself.
    let summaries = vec![
        FileSummary { path: "a".into(), quality: 6, summary: "".into() },
        FileSummary { path: "b".into(), quality: 8, summary: "".into() },
    ];
    assert_eq!(compute_global_quality(&summaries), 7);
}
```

- [ ] **Step 2: Run — verify compile fail**

```
cargo nextest run -p comply review::summary
```

Expected: compile error (`plan_summary`, `SummaryMode`, `FileSummary`, `compute_global_quality` not defined).

- [ ] **Step 3: Implement `plan_summary` + `compute_global_quality` + `generate_tldr`**

`plan_summary`: pure decision function returning `SummaryPlan { mode: SinglePass | MapReduce, groups: Vec<Vec<FileSummary>> }`. Threshold at 15 files (decision log: "map-reduce if > 15"). Splits into 3 approximately-equal groups.

`compute_global_quality`: simple mean of per-file `quality` fields, rounded to nearest `u8`.

`generate_tldr`: async wrapper around the worker call. Wire via the same `worker::invoke` helper used elsewhere.

- [ ] **Step 4: Verify tests PASS + clippy clean**

```
cargo nextest run -p comply review::summary
cargo clippy --all --all-targets -- -D warnings
```

Expected: 3 tests pass, no clippy warnings.

- [ ] **Step 5: Commit**

```
git add src/review/summary.rs src/review/mod.rs
git commit -m "feat(review): Sonnet global summary with map-reduce past 15 files"
```

### M5 exit criteria

- `comply review --last-commit` prints a readable markdown report
- `--format json` produces valid NDJSON
- Exit codes correctly reflect clean / findings / error

---

## M6 — SQLite cache + failure modes

### Task 6.1: `cache.rs` — schema & API

**Files:**
- Create: `src/review/cache.rs`
- Modify: `src/review/mod.rs` (`pub mod cache;`)

Modelled on `src/llm/cache.rs`. New DB file `.comply/review-cache.sqlite` (separate from the linter cache).

```sql
CREATE TABLE IF NOT EXISTS chunks (
  cache_key TEXT PRIMARY KEY,
  findings_json TEXT NOT NULL,
  quality_score INTEGER NOT NULL,
  file_summary TEXT,
  written_at INTEGER NOT NULL
);
```

```rust
pub fn cache_key(
    file_content: &str,
    skills_used: &[String],       // sorted list of skill IDs
    skill_body_hashes: &[String], // sorted list of skill body hashes
    model: &str,
    prompt_version: u32,
) -> String;
```

Key includes `skill_body_hashes` so editing a skill body invalidates (decision 8.1).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn cache_key_changes_when_skill_body_hash_changes() {
    let k1 = cache_key("src", &["react".into()], &["hashA".into()], "sonnet", 1);
    let k2 = cache_key("src", &["react".into()], &["hashB".into()], "sonnet", 1);
    assert_ne!(k1, k2);
}

#[test]
fn cache_key_stable_for_same_inputs() {
    let k1 = cache_key("src", &["react".into()], &["h".into()], "sonnet", 1);
    let k2 = cache_key("src", &["react".into()], &["h".into()], "sonnet", 1);
    assert_eq!(k1, k2);
}

#[test]
fn lookup_miss_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join("c.sqlite")).unwrap();
    assert!(cache.lookup("no-such-key").unwrap().is_none());
}

#[test]
fn store_then_lookup_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(&dir.path().join("c.sqlite")).unwrap();
    let entry = CacheEntry {
        findings_json: "[]".into(),
        quality_score: 8,
        file_summary: Some("ok".into()),
    };
    cache.store("k1", &entry).unwrap();
    let got = cache.lookup("k1").unwrap().unwrap();
    assert_eq!(got.quality_score, 8);
    assert_eq!(got.file_summary.as_deref(), Some("ok"));
}
```

- [ ] **Step 2: Run — verify compile fail**

```
cargo nextest run -p comply review::cache
```

Expected: compile error (`Cache`, `CacheEntry`, `cache_key` not defined).

- [ ] **Step 3: Implement**

- `cache_key(file_content, skills_used, skill_body_hashes, model, prompt_version) -> String`: concatenate a stable serialisation (sort `skills_used` + `skill_body_hashes` lexicographically), SHA256, hex-encode.
- `Cache::open(path: &Path) -> Result<Self>`: opens or creates the SQLite DB with the `chunks` table schema shown above.
- `Cache::lookup(&self, key: &str) -> Result<Option<CacheEntry>>`: SELECT by primary key.
- `Cache::store(&self, key: &str, entry: &CacheEntry) -> Result<()>`: UPSERT with `written_at = unix_ts()`.

Follow `src/llm/cache.rs` for the rusqlite conventions used in this repo.

- [ ] **Step 4: Verify tests PASS + clippy clean**

```
cargo nextest run -p comply review::cache
cargo clippy --all --all-targets -- -D warnings
```

Expected: 4 tests pass, no clippy warnings.

- [ ] **Step 5: Commit**

```
git add src/review/cache.rs src/review/mod.rs
git commit -m "feat(review): SQLite cache with skill-hash-invalidated keys"
```

### Task 6.2: Hook cache into chunk pipeline

**Files:**
- Modify: `src/review/mod.rs` (pipeline around the findings dispatch)

- Before dispatching a findings job: check cache by chunk key. If hit, skip the LLM call.
- After a successful findings result: store under the chunk key.
- `--no-cache` bypasses both read and write.

- [ ] **Step 1: Write failing integration test `tests/review_cache.rs`**

```rust
use assert_cmd::Command;

/// Two runs in a row on the same diff: the second must be materially
/// faster AND report cache hits.
#[test]
fn second_run_uses_cache() {
    let tmp_repo = super::setup_repo_with_one_ts_change();  // reuse M1 helper

    // First run — populates cache.
    Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:one-high")
        .current_dir(tmp_repo.path())
        .assert()
        .code(1);

    // Second run — must emit `cache hits:` line and avoid the stub.
    let out = Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:should-not-be-called")
        .current_dir(tmp_repo.path())
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cache hits:"), "expected cache-hit line: {stderr}");
    assert!(!stderr.contains("should-not-be-called"), "stub must not run on cache hit");
}

#[test]
fn no_cache_flag_bypasses_cache() {
    let tmp_repo = super::setup_repo_with_one_ts_change();
    Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:one-high")
        .current_dir(tmp_repo.path()).assert().code(1);

    // With --no-cache, the stub must run even though cache is populated.
    let out = Command::cargo_bin("comply").unwrap()
        .args(["review", "--working-tree", "--no-cache"])
        .env("COMPLY_REVIEW_WORKER_STUB", "findings:empty")
        .current_dir(tmp_repo.path())
        .output().unwrap();
    assert!(out.status.code() == Some(0), "expected exit 0 with empty findings from stub");
}
```

- [ ] **Step 2: Run — fail (cache not wired)**
- [ ] **Step 3: Wire cache read/write around the worker dispatch loop**
- [ ] **Step 4: Verify PASS + clippy**
- [ ] **Step 5: Commit**

```
git add src/review/mod.rs tests/review_cache.rs
git commit -m "feat(review): cache lookup before dispatch, store after success, --no-cache bypass"
```

### Task 6.3: Rate-limit retry

**Files:**
- Modify: `tools/review-worker.ts`
- Create: `tools/review-worker.test.ts` (Bun test)

Mirror the retry helper in `tools/llm-worker.ts` (the one wrapping the `generateObject` call) — exponential backoff 2s/4s/8s × 3 on `429`, `529`, or `overloaded_error`. Identify it by name in the existing worker, copy the wrapper into `tools/review-worker.ts`, keep the same delays.

- [ ] **Step 1: Write failing test**

```ts
// tools/review-worker.test.ts
import { test, expect } from 'bun:test'
import { withRetry, RETRY_DELAYS_MS } from './review-worker'

test('retry delays match the llm-worker spec', () => {
    expect(RETRY_DELAYS_MS).toEqual([2000, 4000, 8000])
})

test('withRetry retries on 429 then succeeds', async () => {
    let calls = 0
    const fn = async () => {
        calls += 1
        if (calls < 3) {
            const err = new Error('rate limited') as any
            err.statusCode = 429
            throw err
        }
        return 'ok'
    }
    const result = await withRetry(fn, { delaysMs: [1, 2, 4] }) // tiny delays for the test
    expect(result).toBe('ok')
    expect(calls).toBe(3)
})

test('withRetry does not retry on non-retryable errors', async () => {
    let calls = 0
    const fn = async () => { calls += 1; throw new Error('validation failed') }
    await expect(withRetry(fn, { delaysMs: [1] })).rejects.toThrow('validation')
    expect(calls).toBe(1)
})
```

- [ ] **Step 2: Run — expect fail**

```
cd tools && bun test review-worker.test.ts
```

Expected: test errors because `withRetry` / `RETRY_DELAYS_MS` are not yet exported from `review-worker.ts`.

- [ ] **Step 3: Implement**

Export `RETRY_DELAYS_MS = [2000, 4000, 8000]` and a `withRetry(fn, opts?)` helper mirroring `tools/llm-worker.ts`. Wrap the `generateObject` call in it.

- [ ] **Step 4: Verify PASS**

```
cd tools && bun test review-worker.test.ts
```

Expected: 3 passing tests.

- [ ] **Step 5: Commit**

```
git add tools/review-worker.ts tools/review-worker.test.ts
git commit -m "feat(review): exponential retry on 429/529/overloaded with 2s/4s/8s delays"
```

### Task 6.4: Invalid JSON → skip + log

Already in `callModel`'s try/catch (M2 Task 2.3). Confirm Rust side logs `error` entries without aborting the batch.

### Task 6.5: Ctrl-C handling

**Files:**
- Create: `src/review/pipeline_state.rs` (testable state machine)
- Modify: `src/review/mod.rs` (install SIGINT handler + wire to state)

**Design:** split the concern into (a) a pure state machine that is unit-testable and (b) a thin signal-handler that mutates it. This keeps 95 % of the logic out of the hard-to-test async/signal path.

```rust
// src/review/pipeline_state.rs
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct PipelineState {
    interrupted: AtomicBool,
    total_chunks: AtomicUsize,
    completed_chunks: AtomicUsize,
}

impl PipelineState {
    pub fn new(total: usize) -> Arc<Self> {
        let s = Self::default();
        s.total_chunks.store(total, Ordering::SeqCst);
        Arc::new(s)
    }
    pub fn interrupt(&self) { self.interrupted.store(true, Ordering::SeqCst); }
    pub fn is_interrupted(&self) -> bool { self.interrupted.load(Ordering::SeqCst) }
    pub fn can_dispatch_more(&self) -> bool { !self.is_interrupted() }
    pub fn record_completed(&self) { self.completed_chunks.fetch_add(1, Ordering::SeqCst); }
    pub fn progress(&self) -> (usize, usize) {
        (self.completed_chunks.load(Ordering::SeqCst),
         self.total_chunks.load(Ordering::SeqCst))
    }
}
```

Behaviour contract:
1. On SIGINT: set `interrupted = true`. In-flight chunk tasks finish normally (their `Drop` doesn't touch the cache; the *caller* only writes to cache if `is_interrupted() == false` at completion).
2. Dispatch loop checks `can_dispatch_more()` before sending each new chunk.
3. A 10-second grace timer is set after the first SIGINT; on expiry we abort in-flight tasks and emit whatever we have.
4. Output layer prints the banner when `state.is_interrupted()` is true at render time.

- [ ] **Step 1: Unit tests on `PipelineState` (fully deterministic, no signals)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_can_dispatch() {
        let s = PipelineState::new(10);
        assert!(s.can_dispatch_more());
        assert!(!s.is_interrupted());
    }

    #[test]
    fn interrupt_blocks_further_dispatch() {
        let s = PipelineState::new(10);
        s.interrupt();
        assert!(!s.can_dispatch_more());
        assert!(s.is_interrupted());
    }

    #[test]
    fn progress_tracks_completed_chunks() {
        let s = PipelineState::new(5);
        s.record_completed();
        s.record_completed();
        assert_eq!(s.progress(), (2, 5));
    }

    #[test]
    fn interrupt_is_idempotent() {
        let s = PipelineState::new(3);
        s.interrupt();
        s.interrupt();
        assert!(s.is_interrupted());
    }
}
```

- [ ] **Step 2: Integration test with SIGINT (gate behind `#[cfg(unix)]`)**

`tests/review_interrupt.rs`:

```rust
#![cfg(unix)]

use assert_cmd::cargo::CommandCargoExt;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// SIGINT a running review after 1s; expect exit ≤ 12s with the
/// interrupted banner on stderr.
#[test]
fn sigint_produces_partial_results_banner() {
    // Stub worker responds slowly (sleep 30s) so the review is guaranteed
    // to be mid-flight when we send SIGINT. Set via env var consumed by
    // src/review/worker.rs when COMPLY_REVIEW_WORKER_STUB=slow.
    let mut child = Command::cargo_bin("comply").unwrap()
        .args(["review", "--last-commit"])
        .env("COMPLY_REVIEW_WORKER_STUB", "slow")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn comply review");

    std::thread::sleep(Duration::from_millis(1_000));
    kill(Pid::from_raw(child.id() as i32), Signal::SIGINT).unwrap();

    let start = Instant::now();
    let out = child.wait_with_output().expect("wait");
    assert!(start.elapsed() < Duration::from_secs(12), "review should exit within grace period");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("interrupted") || stderr.contains("partial results"),
        "expected interrupted banner in stderr, got: {stderr}"
    );
}
```

Add `nix` as dev-dep with feature `signal`. If `nix` is too heavy for v1, fall back to posting the signal via `libc::kill` directly — either works.

- [ ] **Step 3: Manual smoke test**

```
cargo build --release
./target/release/comply review --last-commit   # press Ctrl-C mid-run
```

Expected: partial findings printed, banner `⚠ interrupted, partial results shown (X/Y chunks)`.

- [ ] **Step 4: Commit**

```
git add src/review/pipeline_state.rs src/review/mod.rs tests/review_interrupt.rs Cargo.toml
git commit -m "feat(review): SIGINT handling with partial results + unit-tested state machine"
```

### Task 6.6: Hard cap 50 files post-triage

After triage, count `NeedsReview` files. If > `config.review().max_files()`, return `ReviewError::DiffTooLarge(n)`. `main.rs` translates that to exit 2 + a friendly message.

### M6 exit criteria

- Cache hit skips LLM call; second run is near-instant for unchanged files
- Editing a skill body invalidates all affected cache entries
- Hard cap enforced
- Ctrl-C prints partial results without garbage

---

## M7 — End-to-end dogfood + tuning

### Task 7.1: Add `--timings` flag (reuse pattern from `comply`)

Per-phase breakdown on stderr: `prefilter: 12ms`, `triage: 2.3s (5 calls)`, `dep_graph: 340ms`, `findings: 8.1s (5 calls)`, `summary: 1.4s`.

### Task 7.2: Log cache hit/miss stats

`cache hits: 3, misses: 2, api calls saved: 3`.

### Task 7.3: Dogfood on 3 real PRs from this repo

Pick branches with varied diffs:
1. `feat/skill-driven-rules` (current, many new files)
2. A recent small refactor PR (low signal expected)
3. A PR that touches core types (high blast radius expected)

Record: time, tokens burned (if visible), findings produced, false positive rate (manual judgment).

### Task 7.4: Tune triage prompt

Iterate on the Haiku prompt to reduce false `NEEDS_REVIEW` (config files, trivial renames that slip through pre-filter). Target: ≥ 70% precision on NEEDS_REVIEW.

### Task 7.5: Tune findings prompt

Iterate for signal quality. Target: ≥ 60% of findings agreed-useful by manual review.

### Task 7.6: Update root `CLAUDE.md` and `README.md`

Document `comply review` usage, env vars, expected behavior.

### Task 7.7: Open a PR merging `feat/comply-review` → `main`

Use the `finishing-branch` skill to verify, then `git-worktrees` if the work happened in a worktree. Ask before pushing.

### M7 exit criteria

- Runs on 3 real PRs without crash
- Markdown report is pleasant to read
- FP rate documented with concrete examples
- CLAUDE.md + README.md updated

---

## Test commands (reference)

```bash
# Full test suite — aim for green after each task
cargo nextest run

# Scoped: only this subsystem
cargo nextest run -p comply review::

# Lint
cargo clippy --all --all-targets -- -D warnings

# Manual end-to-end, α backend
cargo build --release
./target/release/comply review --last-commit

# Manual end-to-end, β backend (opt-in, needs ANTHROPIC_API_KEY)
COMPLY_REVIEW_BACKEND=anthropic ANTHROPIC_API_KEY=sk-... \
  ./target/release/comply review --last-commit
```

---

## Open items (non-blocking, document during implementation)

- Token estimation heuristic (`bytes/4`) — acceptable v1, may want `tiktoken` in v2
- Rust resolver edge cases (`#[path]`, workspace deps, `pub use` re-exports) — punt to v2
- β backend real `cache_control` wiring in `tools/review-worker.ts` — M6 delivers the stub; landing the actual `cache_control` on the skill block needs an Anthropic SDK version check, which we do inline in M6

## References

- Decision log: `docs/plans/2026-04-21-comply-review-llm-decision-log.md`
- Existing linter (for patterns, NOT to be fused): `src/llm/mod.rs`, `tools/llm-worker.ts`
- Diff extraction reference: `src/changed_lines.rs`
- Tree-sitter walker reference: `src/rules/walker.rs`
- Diagnostic model (separate from review): `src/diagnostic.rs`
