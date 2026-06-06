//! comply-ignore parser — scans source for suppression comments + filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>` (em-dash or ` -- `).
//! - **Above-line:** marker is the only thing on the line → suppresses next line.
//! - **Trailing:** marker comes after code on the same line → suppresses current line.
//! - **String literals:** markers inside `"..."`, `'...'`, or `` `...` `` are ignored.
//! - Justification is mandatory; missing → emit `comply-ignore-missing-justification`.

mod line;
mod payload;

use crate::diagnostic::Diagnostic;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Result of parsing comply-ignore comments in a source file.
#[derive(Debug)]
pub struct IgnoreResult {
    /// Map: line number → set of rule ids suppressed on that line. Keyed
    /// this way (instead of HashSet<(line, String)>) so the lookup in
    /// `apply_suppressions` doesn't have to clone the rule_id per check.
    pub suppressions: HashMap<usize, HashSet<String>>,
    /// Set of rule ids suppressed for the entire file via the
    /// `// comply-ignore-file: <rule-id> — <reason>` directive.
    pub file_suppressions: HashSet<String>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut file_suppressions: HashSet<String> = HashSet::new();
    let mut bad_ignores = Vec::new();

    // Strip leading UTF-8 BOM — `is_whitespace` doesn't include U+FEFF, so
    // a line-1 ignore in a BOM-prefixed file would never apply otherwise.
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);

    // Pass 1 — parse every line and remember which lines are themselves
    // marker lines. Needed in pass 2 to forward above-line markers past
    // stacked siblings (ESLint behaviour, rbaumier/comply#22).
    let mut parses: Vec<(usize, line::LineParse)> = Vec::new();
    let mut marker_lines: HashSet<usize> = HashSet::new();
    let mut last_line = 0usize;
    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = idx + 1;
        last_line = line_num;
        if let Some(parsed) = line::parse(path, raw_line, line_num) {
            marker_lines.insert(line_num);
            parses.push((line_num, parsed));
        }
    }

    // Skip JSDoc lines when forwarding above-line markers so a `// comply-ignore` above `/** ... */` still reaches the declaration below (#185).
    let mut jsdoc_lines: HashSet<usize> = HashSet::new();
    {
        let mut in_block = false;
        for (idx, raw_line) in source.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = raw_line.trim_start();
            if !in_block {
                if trimmed.starts_with("/**") {
                    jsdoc_lines.insert(line_num);
                    let after_open = &trimmed[3..];
                    if !after_open.contains("*/") {
                        in_block = true;
                    }
                }
            } else {
                jsdoc_lines.insert(line_num);
                if trimmed.contains("*/") {
                    in_block = false;
                }
            }
        }
    }

    // Pass 2 — apply each parse. Above-line markers whose immediate
    // target is itself a marker line or a JSDoc line walk past those
    // siblings to the first real code line, so stacked markers union
    // their rules onto the same eventual target.
    for (line_num, parsed) in parses {
        if let Some(d) = parsed.bad_ignore {
            bad_ignores.push(d);
        }
        let resolved_target = match parsed.target_line {
            None => None,
            Some(t) if t == line_num => Some(t), // trailing marker
            Some(mut t) => {
                while t <= last_line
                    && (marker_lines.contains(&t) || jsdoc_lines.contains(&t))
                {
                    t += 1;
                }
                Some(t)
            }
        };
        match resolved_target {
            Some(line_no) => {
                let entry = suppressions.entry(line_no).or_default();
                for rule in parsed.rule_ids {
                    entry.insert(rule);
                }
            }
            None => {
                for rule in parsed.rule_ids {
                    file_suppressions.insert(rule);
                }
            }
        }
    }

    IgnoreResult {
        suppressions,
        file_suppressions,
        bad_ignores,
    }
}

/// Filter diagnostics by removing suppressed ones, then append bad-ignore diagnostics.
pub fn apply_suppressions(
    diagnostics: Vec<Diagnostic>,
    path: &Path,
    source: &str,
) -> Vec<Diagnostic> {
    let ignore_result = parse_ignores(path, source);
    let total = diagnostics.len() + ignore_result.bad_ignores.len();
    let mut result: Vec<Diagnostic> = Vec::with_capacity(total);

    for diag in diagnostics {
        let suppressed_at_line = ignore_result
            .suppressions
            .get(&diag.line)
            .is_some_and(|rules| rules.contains(diag.rule_id.as_ref()));
        let suppressed_for_file = ignore_result
            .file_suppressions
            .contains(diag.rule_id.as_ref());
        if !suppressed_at_line && !suppressed_for_file {
            result.push(diag);
        }
    }
    result.extend(ignore_result.bad_ignores);
    result
}

/// Apply comply-ignore suppressions across every discovered file.
///
/// Iterates over every discovered file (not files with diagnostics) so
/// malformed `comply-ignore` comments in clean files are still flagged.
///
/// **Path canonicalization**: oxlint reports paths it canonicalized
/// internally, while the discovery walker returns paths as passed by the
/// user. Without canonicalizing both sides, the HashMap lookup would
/// silently miss for every oxlint diagnostic — completely defeating
/// `comply-ignore` for any oxlint rule.
pub fn apply_to_all(
    diagnostics: Vec<Diagnostic>,
    discovered: &[crate::files::SourceFile],
) -> Vec<Diagnostic> {
    // `canonical_key` is a filesystem syscall (realpath). With tens of
    // thousands of diagnostics spread over a few thousand files, canonicalizing
    // once per diagnostic dominated this phase. Memoize: each distinct path is
    // canonicalized once, then served from the cache. `get(&Path)` borrows, so
    // a cache hit allocates nothing.
    let mut canon_cache: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut canon = |p: &Path| -> PathBuf {
        if let Some(c) = canon_cache.get(p) {
            return c.clone();
        }
        let c = canonical_key(p);
        canon_cache.insert(p.to_path_buf(), c.clone());
        c
    };

    let mut by_file: HashMap<PathBuf, Vec<Diagnostic>> =
        HashMap::with_capacity(diagnostics.len());
    for d in diagnostics {
        let key = canon(d.path.as_ref());
        by_file.entry(key).or_default().push(d);
    }

    // Pair each discovered file with its diagnostics moved out of the map, so
    // the per-file disk read + scan below can run in parallel — each file is
    // fully independent. `into_par_iter().flat_map(..).collect()` preserves the
    // discovered order, so output is identical to the sequential version.
    let mut work: Vec<(&crate::files::SourceFile, Vec<Diagnostic>)> =
        Vec::with_capacity(discovered.len());
    for file in discovered {
        let key = canon(&file.path);
        let file_diags = by_file.remove(&key).unwrap_or_default();
        work.push((file, file_diags));
    }

    let mut result: Vec<Diagnostic> = work
        .into_par_iter()
        .flat_map_iter(|(file, file_diags)| {
            let out: Vec<Diagnostic> = match std::fs::read_to_string(&file.path) {
                // Fast path: a file with no `comply-ignore` marker anywhere can
                // neither suppress a diagnostic nor carry a malformed marker, so
                // the multi-pass line scan in `parse_ignores` is pure waste. One
                // SIMD substring check over the whole file replaces two per-line
                // `find` scans on every line of the repo.
                Ok(src) if !src.contains("comply-ignore") => file_diags,
                Ok(src) => apply_suppressions(file_diags, &file.path, &src),
                Err(e) => {
                    eprintln!("comply: skipping ignore-scan for {}: {e}", file.path.display());
                    file_diags
                }
            };
            out.into_iter()
        })
        .collect();

    // Files not in `discovered` (truly orphaned) pass through unchanged.
    for (_, file_diags) in by_file {
        result.extend(file_diags);
    }
    result
}

/// Canonical path key for HashMap matching. Falls back to the original path
/// if the file no longer exists (canonicalize fails on missing files).
fn canonical_key(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn diag(line: usize, rule_id: &'static str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(Path::new("t.ts")),
            line,
            column: 1,
            rule_id: rule_id.into(),
            message: "test".into(),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn parse_extracts_above_line_suppression() {
        let r = parse_ignores(Path::new("t.ts"), "// comply-ignore: no-throw — ok\nx;");
        assert!(
            r.suppressions
                .get(&2)
                .is_some_and(|s| s.contains("no-throw"))
        );
        assert!(r.bad_ignores.is_empty());
    }

    #[test]
    fn parse_extracts_trailing_suppression() {
        let r = parse_ignores(
            Path::new("t.ts"),
            "throw err; // comply-ignore: no-throw — legacy\n",
        );
        assert!(
            r.suppressions
                .get(&1)
                .is_some_and(|s| s.contains("no-throw"))
        );
    }

    #[test]
    fn missing_justification_emits_diagnostic() {
        let r = parse_ignores(Path::new("t.ts"), "// comply-ignore: no-throw\nx;");
        assert_eq!(r.bad_ignores.len(), 1);
    }

    #[test]
    fn apply_suppressions_removes_matching() {
        let s = "// comply-ignore: no-throw — ok\nthrow err;";
        assert!(apply_suppressions(vec![diag(2, "no-throw")], Path::new("t.ts"), s).is_empty());
    }

    #[test]
    fn apply_suppressions_keeps_unrelated() {
        let s = "// comply-ignore: no-throw — ok\nlet x = 5;";
        assert_eq!(
            apply_suppressions(vec![diag(2, "no-other")], Path::new("t.ts"), s).len(),
            1
        );
    }

    #[test]
    fn file_marker_suppresses_every_line() {
        // Regression for rbaumier/comply#27 — `// comply-ignore-file`
        // must clear diagnostics regardless of line number.
        let s = "// comply-ignore-file: elysia-test-missing-validation — third-party endpoint\nthrow err;\nthrow err;";
        let kept = apply_suppressions(
            vec![
                diag(1, "elysia-test-missing-validation"),
                diag(2, "elysia-test-missing-validation"),
                diag(10, "elysia-test-missing-validation"),
            ],
            Path::new("t.ts"),
            s,
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn file_marker_does_not_silence_other_rules() {
        let s = "// comply-ignore-file: no-throw — ok\nlet x = 1;";
        let kept = apply_suppressions(vec![diag(2, "no-other")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn multi_rule_marker_suppresses_each_rule() {
        // Regression for rbaumier/comply#22 — comma-separated rules.
        let s = "// comply-ignore: rule-a, rule-b — same reason\nthrow err;";
        let kept = apply_suppressions(
            vec![diag(2, "rule-a"), diag(2, "rule-b"), diag(2, "rule-c")],
            Path::new("t.ts"),
            s,
        );
        // rule-a and rule-b suppressed; rule-c remains.
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "rule-c");
    }

    #[test]
    fn stacked_above_line_markers_union_onto_target() {
        // Regression for rbaumier/comply#22 — stacked markers should
        // accumulate, not the closest-wins behaviour ESLint avoids.
        let s = "// comply-ignore: rule-a — A\n// comply-ignore: rule-b — B\nthrow err;";
        let kept = apply_suppressions(
            vec![diag(3, "rule-a"), diag(3, "rule-b")],
            Path::new("t.ts"),
            s,
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn stacked_with_blank_lines_between_markers() {
        // Defensive: blank lines between marker lines must not break
        // the chain — the target is still the first code line.
        let s = "// comply-ignore: rule-a — A\n// comply-ignore: rule-b — B\nthrow err;";
        let kept = apply_suppressions(
            vec![diag(3, "rule-a"), diag(3, "rule-b")],
            Path::new("t.ts"),
            s,
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn marker_above_jsdoc_targets_declaration_below() {
        // Regression for rbaumier/comply#185 — the marker sits above a
        // JSDoc block which itself sits above a declaration. The walk
        // must skip the JSDoc lines and land on the declaration.
        let s = "// comply-ignore: cyclomatic-complexity — exhaustive dispatch.\n\
                 /**\n * JSDoc.\n */\n\
                 export function authorize() {}\n";
        // The function declaration is on line 5.
        let r = parse_ignores(Path::new("t.ts"), s);
        assert!(
            r.suppressions
                .get(&5)
                .is_some_and(|s| s.contains("cyclomatic-complexity")),
            "suppression should target the function line; got {:?}",
            r.suppressions
        );
    }

    #[test]
    fn marker_above_single_line_jsdoc_targets_declaration_below() {
        // A one-line JSDoc still counts — opens and closes on the same line.
        let s = "// comply-ignore: cyclomatic-complexity — reason.\n\
                 /** inline doc */\n\
                 export function authorize() {}\n";
        let r = parse_ignores(Path::new("t.ts"), s);
        assert!(
            r.suppressions
                .get(&3)
                .is_some_and(|s| s.contains("cyclomatic-complexity")),
            "single-line JSDoc must also be walked past; got {:?}",
            r.suppressions
        );
    }

    #[test]
    fn marker_above_jsdoc_does_not_silence_unrelated_line_below_block() {
        // The JSDoc walk only applies to forwarding from a marker — code on
        // lines other than the resolved target stays unaffected.
        let s = "// comply-ignore: rule-a — A\n\
                 /**\n * JSDoc.\n */\n\
                 throw err;\n\
                 throw err;\n";
        // Line 5 should be suppressed for rule-a; line 6 should not.
        let kept = apply_suppressions(
            vec![diag(5, "rule-a"), diag(6, "rule-a")],
            Path::new("t.ts"),
            s,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].line, 6);
    }
}
