//! Diff-scoped diagnostic filtering.
//!
//! Parses `git diff --unified=0` (or `git show -U0`) for the selected
//! scan mode and returns, for each file, the set of line numbers that
//! were added or modified on the `+` side. Used by `--diff-only` to
//! drop diagnostics on pre-existing lines so CI only complains about
//! what the current change introduced.
//!
//! Rules still run on the full file so context-dependent checks work;
//! only the reporting step is filtered.

use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ScanMode;
use crate::diagnostic::Diagnostic;

/// Map from repo-relative file path to the 1-based line numbers that
/// the selected scan mode introduced. `BTreeSet` keeps lookups O(log n)
/// and the structure small for large diffs.
pub type ChangedLines = FxHashMap<PathBuf, BTreeSet<usize>>;

/// Compute the set of changed lines per file for the given scan mode.
/// `ScanMode::All` has no associated diff — returns an empty map and
/// the caller should have already rejected `--diff-only` at the CLI
/// layer. Kept infallible here so callers don't need a separate branch.
pub fn changed_lines(mode: &ScanMode) -> Result<ChangedLines> {
    let output = match mode {
        ScanMode::All(_) => return Ok(FxHashMap::default()),
        ScanMode::WorkingTree => run_git_diff(&["diff", "--unified=0", "--no-color"]),
        ScanMode::Staged => run_git_diff(&["diff", "--cached", "--unified=0", "--no-color"]),
        ScanMode::LastCommit => {
            run_git_diff(&["diff", "--unified=0", "--no-color", "HEAD~1", "HEAD"])
        }
        ScanMode::Commit(sha) => {
            run_git_diff(&["show", "--unified=0", "--no-color", "--pretty=format:", sha])
        }
        ScanMode::Range(from, to) => run_git_diff(&[
            "diff",
            "--unified=0",
            "--no-color",
            from.as_str(),
            to.as_str(),
        ]),
    }?;
    Ok(parse_unified_diff(&output))
}

/// True if `diag` falls on a line that appears in the changed-lines set
/// for its file. Missing file entry → dropped (the file didn't change).
#[must_use]
pub fn diag_in_diff(diag: &Diagnostic, changed: &ChangedLines) -> bool {
    changed
        .get(diag.path.as_ref() as &Path)
        .is_some_and(|lines| lines.contains(&diag.line))
}

/// Return the absolute path to the git repository root (`git rev-parse
/// --show-toplevel`). `None` if we're not in a git repo or git isn't
/// available — callers treat that as "no prefix stripping needed".
#[must_use]
pub fn git_repo_root() -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?;
    Some(PathBuf::from(path.trim()))
}

/// Run git and hand back its stdout as raw bytes.
///
/// A diff embeds the content of every changed file, so its bytes are only
/// as decodable as the files themselves — a latin-1 CSV fixture in the
/// range puts non-UTF-8 bytes in the output. Decoding happens later, per
/// header line, so a file comply does not even lint cannot fail the run.
fn run_git_diff(args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    let out = cmd
        .output()
        .context("failed to invoke git for --diff-only — is git on PATH?")?;
    if !out.status.success() {
        bail!(
            "git {args:?} failed (exit {}): {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// Parse unified-diff output into `(path → changed line numbers)`.
/// Recognises the two file-delimiter forms git emits:
///   `diff --git a/FOO b/FOO`  → used for in-place edits
///   `+++ b/FOO`               → the canonical "destination" path line
/// We key off `+++` because `diff --git` headers can lie for renames
/// (they show the OLD path). The destination path is always accurate.
///
/// A `+++ /dev/null` line means the file was deleted — we drop it.
///
/// Hunk headers are `@@ -X,Y +A,B @@` where `A` is the 1-based start
/// line on the new side and `B` is the count (default 1 when omitted,
/// 0 for pure-deletion hunks which we skip).
///
/// Works on bytes: everything between the header lines is file content
/// git copied verbatim, and a file whose language comply does not lint
/// (a latin-1 CSV fixture, say) must not be able to break the scan.
/// Only the two header forms above are decoded as text.
fn parse_unified_diff(diff: &[u8]) -> ChangedLines {
    let mut out: ChangedLines = FxHashMap::default();
    let mut current: Option<PathBuf> = None;

    for line in diff.split(|&byte| byte == b'\n') {
        if let Some(rest) = line.strip_prefix(b"+++ ") {
            current = parse_destination_path(rest);
            continue;
        }
        let Some(hunk) = line.strip_prefix(b"@@ ") else {
            continue;
        };
        let Some(path) = current.as_ref() else {
            continue;
        };
        let Some((start, count)) = parse_hunk_header(hunk) else {
            continue;
        };
        if count == 0 {
            continue;
        }
        let entry = out.entry(path.clone()).or_default();
        for n in start..start + count {
            entry.insert(n);
        }
    }
    out
}

/// Decode the destination path from a `+++ ` line — the bytes after the
/// prefix, e.g. `b/src/app.ts\t2024-01-01`.
///
/// The path is the one part of a diff comply needs as text. A path git
/// could not encode as UTF-8 names a file our own discovery pass already
/// refuses (`files::parse_git_output`), so skipping its hunks costs no
/// coverage — and skipping beats aborting the run that gates the commit.
fn parse_destination_path(rest: &[u8]) -> Option<PathBuf> {
    let Ok(text) = std::str::from_utf8(rest) else {
        eprintln!(
            "comply: --diff-only skipped a file whose diff path is not valid UTF-8: {}",
            String::from_utf8_lossy(rest).trim()
        );
        return None;
    };
    let raw = text.split('\t').next()?.trim();
    if raw == "/dev/null" {
        return None;
    }
    Some(PathBuf::from(
        raw.strip_prefix("b/").unwrap_or(raw).to_string(),
    ))
}

/// Extract `(start, count)` from a unified-diff hunk header body.
/// Input is the bytes after `@@ `, e.g. `-12,3 +14,2 @@ fn foo`.
///
/// Only the range text ahead of the closing ` @@` is decoded. What
/// follows is the context snippet git lifts straight out of the file,
/// which carries the file's own encoding — reading it as UTF-8 would
/// make an unlintable neighbour's bytes decide whether this hunk parses.
fn parse_hunk_header(body: &[u8]) -> Option<(usize, usize)> {
    let close = body.windows(3).position(|window| window == b" @@")?;
    let ranges = std::str::from_utf8(&body[..close]).ok()?;
    let plus = ranges.split_whitespace().find(|t| t.starts_with('+'))?;
    let numbers = plus.strip_prefix('+')?;
    let mut it = numbers.split(',');
    let start = it.next()?.parse().ok()?;
    let count = it.next().map_or(Some(1), |c| c.parse().ok())?;
    Some((start, count))
}

/// Filter `diagnostics` in place, keeping only those that land on
/// lines marked as changed. Clears the input if `changed` is empty.
pub fn retain_in_diff(diagnostics: &mut Vec<Diagnostic>, changed: &ChangedLines) {
    diagnostics.retain(|d| diag_in_diff(d, changed));
}

/// Normalise a diagnostic's path to repo-relative form for lookup in
/// the `ChangedLines` map. Git emits repo-relative paths; diagnostics
/// may carry absolute paths (rules get a `&Path` that can be either).
/// Strips the `repo_root` prefix when present; otherwise returns the
/// path unchanged.
#[must_use]
pub fn normalise_path(path: &Path, repo_root: &Path) -> PathBuf {
    path.strip_prefix(repo_root)
        .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_hunk() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,0 +2,3 @@
+added line 2
+added line 3
+added line 4
";
        let got = parse_unified_diff(diff.as_bytes());
        let lines = got.get(&PathBuf::from("foo.rs")).expect("file present");
        assert_eq!(lines.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn parses_multiple_hunks_same_file() {
        let diff = "\
+++ b/bar.rs
@@ -1,0 +1,1 @@
+added top
@@ -10,0 +20,2 @@
+added 20
+added 21
";
        let got = parse_unified_diff(diff.as_bytes());
        let lines = got.get(&PathBuf::from("bar.rs")).expect("file present");
        assert_eq!(lines.iter().copied().collect::<Vec<_>>(), vec![1, 20, 21]);
    }

    #[test]
    fn single_line_hunk_without_count_defaults_to_one() {
        // Unified diff omits the count when it equals 1 — `+42 @@` means
        // one line added starting at line 42. The parser must treat the
        // missing count as 1, not 0, or we'd silently drop single-line
        // additions.
        let diff = "\
+++ b/baz.rs
@@ -1,0 +42 @@
+single
";
        let got = parse_unified_diff(diff.as_bytes());
        let lines = got.get(&PathBuf::from("baz.rs")).expect("file present");
        assert_eq!(lines.iter().copied().collect::<Vec<_>>(), vec![42]);
    }

    #[test]
    fn pure_deletion_hunk_emits_no_lines() {
        let diff = "\
+++ b/del.rs
@@ -5,3 +4,0 @@
-removed
-removed
-removed
";
        let got = parse_unified_diff(diff.as_bytes());
        assert!(!got.contains_key(&PathBuf::from("del.rs")));
    }

    #[test]
    fn skips_dev_null_destination() {
        let diff = "\
+++ /dev/null
@@ -1,5 +0,0 @@
-whole
-file
-deleted
";
        let got = parse_unified_diff(diff.as_bytes());
        assert!(got.is_empty());
    }

    #[test]
    fn handles_multiple_files() {
        let diff = "\
+++ b/a.rs
@@ -1,0 +1,1 @@
+a
+++ b/b.rs
@@ -1,0 +5,2 @@
+b1
+b2
";
        let got = parse_unified_diff(diff.as_bytes());
        let a = got.get(&PathBuf::from("a.rs")).expect("a.rs present");
        let b = got.get(&PathBuf::from("b.rs")).expect("b.rs present");
        assert_eq!(a.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(b.iter().copied().collect::<Vec<_>>(), vec![5, 6]);
    }

    #[test]
    fn ignores_lines_outside_hunk_headers() {
        let diff = "\
diff --git a/x.rs b/x.rs
--- a/x.rs
+++ b/x.rs
@@ -1,0 +1,1 @@
+ real
 context (unified=0 should not emit this, but tolerate it)
";
        let got = parse_unified_diff(diff.as_bytes());
        let lines = got.get(&PathBuf::from("x.rs")).expect("x.rs present");
        assert_eq!(lines.iter().copied().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn non_utf8_diff_body_does_not_drop_the_run_issue_8490() {
        // A CP1252 CSV fixture (0xE9 = `é`) committed on purpose so a reader's
        // latin-1 fallback stays covered. Its bytes land in the diff body of
        // every `--range … --diff-only` run touching it; the sibling `.ts`
        // change must still be attributed.
        let mut diff = Vec::new();
        diff.extend_from_slice(
            b"diff --git a/a.ts b/a.ts\n\
              --- a/a.ts\n\
              +++ b/a.ts\n\
              @@ -1 +1 @@\n\
              -export const x = 1;\n\
              +export const x = 2;\n\
              diff --git a/fixture.csv b/fixture.csv\n\
              new file mode 100644\n\
              --- /dev/null\n\
              +++ b/fixture.csv\n\
              @@ -0,0 +1,2 @@\n",
        );
        diff.extend_from_slice(b"+Nom;D\xe9signation\r\n");
        diff.extend_from_slice(b"+A;B\r\n");

        let got = parse_unified_diff(&diff);
        let ts = got.get(&PathBuf::from("a.ts")).expect("a.ts present");
        assert_eq!(ts.iter().copied().collect::<Vec<_>>(), vec![1]);
        let csv = got.get(&PathBuf::from("fixture.csv")).expect("csv present");
        assert_eq!(csv.iter().copied().collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn non_utf8_diff_body_keeps_the_staged_gate_issue_8502() {
        // Byte-for-byte the `git diff --cached --unified=0` of the issue's
        // reproducer: a latin-1 CSV appended to alongside a staged `.ts` line.
        // The gate must report the `.ts` line, which is its entire job.
        //
        // The trailing hunk-header context is where git puts the line before
        // the hunk; git strips the unprintable byte out of it (`Caf`, not
        // `Caf<0xE9>`). The third file below drops that assumption: a raw
        // 0xE9 in the context tail must not cost the file its hunk either.
        let mut diff = Vec::new();
        diff.extend_from_slice(
            b"diff --git a/a.ts b/a.ts\n\
              --- a/a.ts\n\
              +++ b/a.ts\n\
              @@ -1,0 +2 @@ export const one = 1;\n\
              +export const two = a ? b ? 1 : 2 : 3;\n\
              diff --git a/sample.csv b/sample.csv\n\
              --- a/sample.csv\n\
              +++ b/sample.csv\n\
              @@ -2,0 +3 @@ Caf\n",
        );
        diff.extend_from_slice(b"+Th\xe9\n");
        diff.extend_from_slice(b"+++ b/raw.csv\n@@ -2,0 +9 @@ Caf\xe9\n+Th\xe9\n");

        let got = parse_unified_diff(&diff);
        let ts = got.get(&PathBuf::from("a.ts")).expect("a.ts present");
        assert_eq!(ts.iter().copied().collect::<Vec<_>>(), vec![2]);
        let csv = got.get(&PathBuf::from("sample.csv")).expect("csv present");
        assert_eq!(csv.iter().copied().collect::<Vec<_>>(), vec![3]);
        let raw = got.get(&PathBuf::from("raw.csv")).expect("raw.csv present");
        assert_eq!(raw.iter().copied().collect::<Vec<_>>(), vec![9]);
    }

    #[test]
    fn non_utf8_path_skips_only_that_file() {
        // Undecodable *path*: that file alone loses its hunks (comply's file
        // discovery refuses such paths anyway), while its neighbours keep theirs.
        let mut diff = Vec::new();
        diff.extend_from_slice(b"+++ b/caf\xe9.csv\n@@ -0,0 +1 @@\n+x\n");
        diff.extend_from_slice(b"+++ b/ok.ts\n@@ -0,0 +7 @@\n+y\n");

        let got = parse_unified_diff(&diff);
        assert_eq!(got.len(), 1, "only the decodable path survives: {got:?}");
        let ok = got.get(&PathBuf::from("ok.ts")).expect("ok.ts present");
        assert_eq!(ok.iter().copied().collect::<Vec<_>>(), vec![7]);
    }

    #[test]
    fn diag_in_diff_matches_changed_line() {
        use crate::diagnostic::Severity;
        let diff = "+++ b/m.rs\n@@ -0,0 +3,2 @@\n+a\n+b\n";
        let changed = parse_unified_diff(diff.as_bytes());
        let on_hit = Diagnostic {
            path: std::sync::Arc::from(std::path::Path::new("m.rs")),
            line: 4,
            column: 1,
            rule_id: "r".into(),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        };
        let on_miss = Diagnostic {
            path: std::sync::Arc::from(std::path::Path::new("m.rs")),
            line: 10,
            column: 1,
            rule_id: "r".into(),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        };
        assert!(diag_in_diff(&on_hit, &changed));
        assert!(!diag_in_diff(&on_miss, &changed));
    }

    #[test]
    fn diag_in_diff_drops_file_not_in_diff() {
        use crate::diagnostic::Severity;
        let changed: ChangedLines = FxHashMap::default();
        let diag = Diagnostic {
            path: std::sync::Arc::from(std::path::Path::new("nope.rs")),
            line: 1,
            column: 1,
            rule_id: "r".into(),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        };
        assert!(!diag_in_diff(&diag, &changed));
    }

    #[test]
    fn retain_in_diff_drops_stale_diagnostics() {
        use crate::diagnostic::Severity;
        let diff = "+++ b/k.rs\n@@ -0,0 +5,1 @@\n+new\n";
        let changed = parse_unified_diff(diff.as_bytes());
        let mut diags = vec![
            Diagnostic {
                path: std::sync::Arc::from(std::path::Path::new("k.rs")),
                line: 5,
                column: 1,
                rule_id: "keep".into(),
                message: String::new(),
                severity: Severity::Error,
                span: None,
            },
            Diagnostic {
                path: std::sync::Arc::from(std::path::Path::new("k.rs")),
                line: 99,
                column: 1,
                rule_id: "drop".into(),
                message: String::new(),
                severity: Severity::Error,
                span: None,
            },
        ];
        retain_in_diff(&mut diags, &changed);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "keep");
    }

    #[test]
    fn normalise_path_strips_repo_root() {
        let got = normalise_path(Path::new("/repo/src/f.rs"), Path::new("/repo"));
        assert_eq!(got, PathBuf::from("src/f.rs"));
    }

    #[test]
    fn normalise_path_leaves_unrelated_paths() {
        let got = normalise_path(Path::new("src/f.rs"), Path::new("/repo"));
        assert_eq!(got, PathBuf::from("src/f.rs"));
    }
}
