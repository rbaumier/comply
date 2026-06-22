//! comply-ignore parser — scans source for suppression comments + filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>` (em-dash or ` -- `).
//! - **Above-line:** marker is the only thing on the line → suppresses next line.
//! - **Trailing:** marker comes after code on the same line → suppresses current line.
//! - **String literals:** markers inside `"..."`, `'...'`, or `` `...` `` are ignored.
//! - Justification is mandatory; missing → emit `comply-ignore-missing-justification`.

mod eslint_config;
mod line;
mod payload;

use crate::diagnostic::Diagnostic;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

/// Result of parsing comply-ignore comments in a source file.
#[derive(Debug)]
pub struct IgnoreResult {
    /// Map: line number → set of rule ids suppressed on that line. Keyed
    /// this way (instead of HashSet<(line, String)>) so the lookup in
    /// `apply_suppressions` doesn't have to clone the rule_id per check.
    pub suppressions: FxHashMap<usize, FxHashSet<String>>,
    /// Set of rule ids suppressed for the entire file via the
    /// `// comply-ignore-file: <rule-id> — <reason>` directive.
    pub file_suppressions: FxHashSet<String>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// True when `source` might carry a suppression directive worth a full scan: a
/// `comply-ignore` marker or an ESLint inline config comment (which always
/// mentions `eslint`). A file with neither substring can suppress nothing, so
/// callers skip the per-line parse entirely. One SIMD substring check each.
#[must_use]
pub fn has_suppression_marker(source: &str) -> bool {
    source.contains("comply-ignore") || source.contains("eslint")
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions: FxHashMap<usize, FxHashSet<String>> = FxHashMap::default();
    let mut file_suppressions: FxHashSet<String> = FxHashSet::default();
    let mut bad_ignores = Vec::new();

    // Strip leading UTF-8 BOM — `is_whitespace` doesn't include U+FEFF, so
    // a line-1 ignore in a BOM-prefixed file would never apply otherwise.
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);

    // Pass 1 — parse every line and remember which lines are themselves
    // marker lines. Needed in pass 2 to forward above-line markers past
    // stacked siblings (ESLint behaviour, rbaumier/comply#22).
    let mut parses: Vec<(usize, line::LineParse)> = Vec::new();
    let mut marker_lines: FxHashSet<usize> = FxHashSet::default();
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
    let mut jsdoc_lines: FxHashSet<usize> = FxHashSet::default();
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

    // ESLint inline config comments (`/* eslint <rule>: 0 */`) turn a rule off
    // for the file. Treat each off-severity rule as a file-level suppression,
    // honoring the same syntax codegen output (AWS SDK Smithy, etc.) relies on.
    for rule in eslint_config::off_rules(source) {
        file_suppressions.insert(rule);
    }

    IgnoreResult {
        suppressions,
        file_suppressions,
        bad_ignores,
    }
}

/// Sibling ids whose `comply-ignore` directive also suppresses `rule_id`. Two
/// cases share this map:
///
/// 1. **Shared-intent siblings.** `no-clones` and `no-duplicate-type-definition`
///    both flag the exact same intentional structural duplication, so one
///    acknowledgement covers both rather than forcing two stacked markers.
/// 2. **Canonicalised aliases (#5768).** Each value-typed rule below is enforced
///    under a single canonical id (the duplicate oxlint-passthrough / tsgolint
///    backends were de-registered). A directive that still cites a former alias id
///    keeps suppressing the canonical finding now emitted in its place.
fn suppression_aliases(rule_id: &str) -> &'static [&'static str] {
    match rule_id {
        "no-duplicate-type-definition" => &["no-clones"],
        "ts-no-explicit-any" => &["typescript/no-explicit-any", "no-explicit-any"],
        "ts-no-inferrable-types" => &["no-inferrable-types"],
        "promise-prefer-await-to-then" => &["promise/prefer-await-to-then"],
        "consistent-type-imports" => &["typescript/consistent-type-imports"],
        _ => &[],
    }
}

/// Whether `rule_id` is suppressed within `suppressed` — directly or because a
/// sibling rule that covers it (see `suppression_aliases`) is present.
fn is_suppressed(rule_id: &str, suppressed: &FxHashSet<String>) -> bool {
    suppressed.contains(rule_id)
        || suppression_aliases(rule_id)
            .iter()
            .any(|alias| suppressed.contains(*alias))
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
            .is_some_and(|rules| is_suppressed(diag.rule_id.as_ref(), rules));
        let suppressed_for_file =
            is_suppressed(diag.rule_id.as_ref(), &ignore_result.file_suppressions);
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
    clean_files: &FxHashSet<PathBuf>,
) -> Vec<Diagnostic> {
    // Group diagnostics by their as-reported path. The in-process engine and
    // clone detector report the discovery path verbatim (a cloned `Arc<Path>`),
    // so this raw match needs no syscall. Keyed by `Arc<Path>` so grouping is a
    // refcount bump, not a path allocation.
    let mut by_raw: FxHashMap<std::sync::Arc<Path>, Vec<Diagnostic>> =
        FxHashMap::with_capacity_and_hasher(diagnostics.len(), Default::default());
    for d in diagnostics {
        by_raw.entry(std::sync::Arc::clone(&d.path)).or_default().push(d);
    }

    // Pair each discovered file with its diagnostics moved out of the map, so
    // the per-file disk read + scan below can run in parallel — each file is
    // fully independent. `into_par_iter().flat_map(..).collect()` preserves the
    // discovered order, so output is identical to the sequential version.
    let mut work: Vec<(&crate::files::SourceFile, Vec<Diagnostic>)> =
        Vec::with_capacity(discovered.len());
    for file in discovered {
        let file_diags = by_raw.remove(file.path.as_path()).unwrap_or_default();
        work.push((file, file_diags));
    }

    // Anything still in `by_raw` had a path that didn't match a discovered file
    // verbatim — the only producer of such paths is an external linter that
    // canonicalized them (oxlint). `canonical_key` is a `realpath` syscall, so
    // this reconciliation is skipped entirely when every path matched above
    // (e.g. `--comply-only`, where no external linter runs), sparing one
    // syscall per discovered file.
    let mut orphans: Vec<Diagnostic> = Vec::new();
    if !by_raw.is_empty() {
        let mut by_canon: FxHashMap<PathBuf, Vec<Diagnostic>> = FxHashMap::default();
        for (raw, diags) in by_raw.drain() {
            by_canon.entry(canonical_key(&raw)).or_default().extend(diags);
        }
        for (file, file_diags) in &mut work {
            if let Some(extra) = by_canon.remove(&canonical_key(&file.path)) {
                file_diags.extend(extra);
            }
        }
        for diags in by_canon.into_values() {
            orphans.extend(diags);
        }
    }

    let mut result: Vec<Diagnostic> = work
        .into_par_iter()
        .flat_map_iter(|(file, file_diags)| {
            // The engine already read this file and saw no suppression marker —
            // it can carry neither a suppression nor a malformed marker, so skip
            // the re-read. Equivalent to the fast path below.
            if clean_files.contains(&file.path) {
                return file_diags.into_iter();
            }
            let out: Vec<Diagnostic> = match std::fs::read_to_string(&file.path) {
                // Fast path: a file with no suppression marker anywhere can
                // neither suppress a diagnostic nor carry a malformed marker, so
                // the multi-pass line scan in `parse_ignores` is pure waste. One
                // SIMD substring check over the whole file replaces two per-line
                // `find` scans on every line of the repo.
                Ok(src) if !has_suppression_marker(&src) => file_diags,
                Ok(src) => apply_suppressions(file_diags, &file.path, &src),
                Err(e) => {
                    eprintln!("comply: skipping ignore-scan for {}: {e}", file.path.display());
                    file_diags
                }
            };
            out.into_iter()
        })
        .collect();

    // Diagnostics that matched no discovered file (truly orphaned) pass through
    // unchanged.
    result.extend(orphans);
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
    fn canonicalized_alias_directive_suppresses_canonical_finding() {
        // Regression for rbaumier/comply#5768 — the duplicate oxlint-passthrough
        // and tsgolint backends were de-registered so each check now emits one
        // finding under its canonical id. A pre-existing directive that still
        // cites a former alias id must keep suppressing that canonical finding.
        let cases: [(&'static str, &'static str); 5] = [
            ("ts-no-explicit-any", "no-explicit-any"),
            ("ts-no-explicit-any", "typescript/no-explicit-any"),
            ("ts-no-inferrable-types", "no-inferrable-types"),
            ("promise-prefer-await-to-then", "promise/prefer-await-to-then"),
            ("consistent-type-imports", "typescript/consistent-type-imports"),
        ];
        for (canonical, alias) in cases {
            let source = format!("// comply-ignore: {alias} — pre-existing\nlet x = 5;");
            assert!(
                apply_suppressions(vec![diag(2, canonical)], Path::new("t.ts"), &source).is_empty(),
                "directive `{alias}` should suppress canonical finding `{canonical}`",
            );
        }
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
    fn no_clones_ignore_also_suppresses_duplicate_type_definition() {
        // #4571 — a span the author deliberately keeps duplicated and documents
        // with `comply-ignore: no-clones` must not be re-flagged by the sibling
        // `no-duplicate-type-definition`, which reports the same duplication.
        let s = "// comply-ignore: no-clones — per-route sort union differs\n\
                 type MockSearch = { page: number };\n";
        let kept = apply_suppressions(
            vec![diag(2, "no-duplicate-type-definition")],
            Path::new("t.ts"),
            s,
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn no_clones_ignore_does_not_suppress_unrelated_rules() {
        // The alias is one-directional and narrow: `no-clones` covers only its
        // structural sibling, never arbitrary rules on the same line.
        let s = "// comply-ignore: no-clones — intentional\nlet x = 1;\n";
        let kept = apply_suppressions(vec![diag(2, "no-throw")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "no-throw");
    }

    #[test]
    fn duplicate_type_definition_ignore_does_not_suppress_no_clones() {
        // Aliasing does not run the other way: ignoring the type rule must not
        // silence `no-clones`, which governs a broader set of duplications.
        let s = "// comply-ignore: no-duplicate-type-definition — ok\ntype T = { a: number };\n";
        let kept = apply_suppressions(vec![diag(2, "no-clones")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "no-clones");
    }

    #[test]
    fn eslint_config_comment_zero_severity_suppresses_whole_file() {
        // #5510 — AWS SDK Smithy codegen heads a schema file with
        // `/* eslint no-var: 0 */`; comply must honor it for the whole file.
        let s = "/* eslint no-var: 0 */\nexport var S3ServiceException = [-3];\n";
        let kept = apply_suppressions(
            vec![diag(2, "no-var"), diag(2, "no-magic-numbers")],
            Path::new("schemas_0.ts"),
            s,
        );
        // Only no-var was set to 0; no-magic-numbers stays flagged.
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "no-magic-numbers");
    }

    #[test]
    fn eslint_config_comment_multiple_off_rules_suppress_each() {
        let s = "/* eslint no-var: 0, no-magic-numbers: 0 */\nexport var x = [-3];\n";
        let kept = apply_suppressions(
            vec![diag(2, "no-var"), diag(2, "no-magic-numbers")],
            Path::new("schemas_0.ts"),
            s,
        );
        assert!(kept.is_empty());
    }

    #[test]
    fn eslint_config_comment_non_zero_severity_does_not_suppress() {
        // A rule left at error severity must keep firing.
        let s = "/* eslint no-var: 2 */\nexport var x = 1;\n";
        let kept = apply_suppressions(vec![diag(2, "no-var")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "no-var");
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
