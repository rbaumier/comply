//! Suppression parser — scans source for suppression comments + filters diagnostics.
//!
//! comply's own format is `// comply-ignore: <rule-id> — <justification>`
//! (em-dash or ` -- `):
//! - **Above-line:** marker is the only thing on the line → suppresses next line.
//! - **Trailing:** marker comes after code on the same line → suppresses current line.
//! - **String literals:** markers inside `"..."`, `'...'`, or `` `...` `` are ignored.
//! - Justification is mandatory; missing → emit `comply-ignore-missing-justification`.
//!
//! ESLint directives written by the author of the scanned project are honored
//! for the comply rule that re-implements the named ESLint rule:
//! `// eslint-disable-line rule-a`, `// eslint-disable-next-line rule-a` and
//! the `/* eslint rule-a: 0 */` config comment. A directive naming no rule
//! suppresses nothing.

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

/// Every directive found in `source`, with the line bookkeeping the target
/// resolution needs.
struct ScannedDirectives {
    /// Each parsed directive, keyed by the line it sits on.
    parses: Vec<(usize, line::LineParse)>,
    /// Lines carrying nothing but a directive comment. An above-line marker
    /// forwards past these to reach the code it was aimed at (ESLint
    /// behaviour, rbaumier/comply#22). A line whose directive trails code is
    /// not in this set: the walk must stop on it, since that code is the
    /// target.
    marker_lines: FxHashSet<usize>,
    /// Number of lines in the file, which bounds the forwarding walk.
    last_line: usize,
}

fn scan_directives(path: &Path, source: &str) -> ScannedDirectives {
    let mut parses = Vec::new();
    let mut marker_lines = FxHashSet::default();
    let mut last_line = 0;
    for (idx, raw_line) in source.lines().enumerate() {
        last_line = idx + 1;
        if let Some(parsed) = line::parse(path, raw_line, last_line) {
            if parsed.is_comment_only_line {
                marker_lines.insert(last_line);
            }
            parses.push((last_line, parsed));
        }
    }
    ScannedDirectives { parses, marker_lines, last_line }
}

/// Lines covered by a JSDoc block. Forwarding an above-line marker skips them
/// so a marker above `/** ... */` still reaches the declaration below (#185).
fn jsdoc_lines(source: &str) -> FxHashSet<usize> {
    let mut lines: FxHashSet<usize> = FxHashSet::default();
    let mut in_block = false;
    for (idx, raw_line) in source.lines().enumerate() {
        let trimmed = raw_line.trim_start();
        let after_open = trimmed.strip_prefix("/**");
        if !in_block && after_open.is_none() {
            continue;
        }
        lines.insert(idx + 1);
        in_block = !after_open.unwrap_or(trimmed).contains("*/");
    }
    lines
}

/// Parse every suppression directive in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions: FxHashMap<usize, FxHashSet<String>> = FxHashMap::default();
    let mut file_suppressions: FxHashSet<String> = FxHashSet::default();
    let mut bad_ignores = Vec::new();

    // Strip leading UTF-8 BOM — `is_whitespace` doesn't include U+FEFF, so
    // a line-1 ignore in a BOM-prefixed file would never apply otherwise.
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);

    let ScannedDirectives { parses, marker_lines, last_line } = scan_directives(path, source);
    let jsdoc_lines = jsdoc_lines(source);

    // Apply each parse. An above-line marker whose immediate target is itself
    // a marker line or a JSDoc line walks past those siblings to the first
    // real code line, so stacked markers union their rules onto one target.
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
                    insert_suppressed_name(entry, rule);
                }
            }
            None => {
                for rule in parsed.rule_ids {
                    insert_suppressed_name(&mut file_suppressions, rule);
                }
            }
        }
    }

    // ESLint inline config comments (`/* eslint <rule>: 0 */`) turn a rule off
    // for the file. Treat each off-severity rule as a file-level suppression,
    // honoring the same syntax codegen output (AWS SDK Smithy, etc.) relies on.
    for rule in eslint_config::off_rules(source) {
        insert_suppressed_name(&mut file_suppressions, rule);
    }

    IgnoreResult {
        suppressions,
        file_suppressions,
        bad_ignores,
    }
}

/// Record a rule name a directive turned off, plus its unprefixed form when it
/// carries a plugin scope.
///
/// Which prefix a directive uses is the scanned project's choice — oxlint
/// writes `typescript/no-explicit-any` where ESLint writes
/// `@typescript-eslint/no-explicit-any` — so the unprefixed name is the only
/// key that matches across configs. `is_suppressed` looks rules up by it.
fn insert_suppressed_name(suppressed: &mut FxHashSet<String>, name: String) {
    let bare = crate::rules::meta_registry::unprefixed_rule_name(&name);
    if bare.len() != name.len() {
        suppressed.insert(bare.to_string());
    }
    suppressed.insert(name);
}

/// Sibling ids whose `comply-ignore` directive also suppresses `rule_id`.
/// `no-clones` and `no-duplicate-type-definition` both flag the exact same
/// intentional structural duplication, so one acknowledgement covers both
/// rather than forcing two stacked markers.
fn suppression_aliases(rule_id: &str) -> &'static [&'static str] {
    match rule_id {
        "no-duplicate-type-definition" => &["no-clones"],
        _ => &[],
    }
}

/// Whether `rule_id` is suppressed within `suppressed`. Three ways to match:
/// the id itself, a sibling rule that covers it (see `suppression_aliases`),
/// or the ESLint rule this one re-implements — which is how a directive
/// written for ESLint (`@typescript-eslint/no-empty-function`) reaches the
/// comply rule enforcing it (`ts-no-empty-function`).
fn is_suppressed(rule_id: &str, suppressed: &FxHashSet<String>) -> bool {
    if suppressed.is_empty() {
        return false;
    }
    suppressed.contains(rule_id)
        || suppression_aliases(rule_id)
            .iter()
            .any(|alias| suppressed.contains(*alias))
        || crate::rules::meta_registry::upstream_eslint_rule(rule_id)
            .is_some_and(|upstream| suppressed.contains(upstream))
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
    // (e.g. a project with no TypeScript, where oxlint never runs), sparing
    // one syscall per discovered file.
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
    fn eslint_disable_next_line_suppresses_the_equivalent_comply_rule() {
        // Regression for rbaumier/comply#1633 — the sveltejs/kit example. The
        // author already told ESLint these expressions are deliberate.
        let s = "untrack(() => {\n\
                 // eslint-disable-next-line @typescript-eslint/no-unused-expressions\n\
                 params.x;\n\
                 });\n";
        let kept = apply_suppressions(
            vec![diag(3, "ts-no-unused-expressions")],
            Path::new("t.ts"),
            s,
        );
        assert!(kept.is_empty(), "expected the directive to be honored, got {kept:?}");
    }

    #[test]
    fn eslint_disable_line_suppresses_its_own_line() {
        let s = "export function noop() {} // eslint-disable-line @typescript-eslint/no-empty-function\n";
        let kept = apply_suppressions(vec![diag(1, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert!(kept.is_empty());
    }

    #[test]
    fn eslint_disable_accepts_every_prefix_form_of_the_same_rule() {
        // A project's ESLint config decides the prefix its directives carry.
        for named in [
            "@typescript-eslint/no-empty-function",
            "typescript/no-empty-function",
            "no-empty-function",
            "ts-no-empty-function",
        ] {
            let s = format!("// eslint-disable-next-line {named}\nexport function noop() {{}}\n");
            let kept =
                apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), &s);
            assert!(kept.is_empty(), "directive `{named}` should suppress the rule");
        }
    }

    #[test]
    fn eslint_disable_naming_another_rule_suppresses_nothing() {
        let s = "// eslint-disable-next-line @typescript-eslint/no-explicit-any\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rule_id.as_ref(), "ts-no-empty-function");
    }

    #[test]
    fn eslint_disable_without_a_rule_list_suppresses_nothing() {
        // A blanket disable is another linter's decision about its own rules;
        // it says nothing about the comply rules firing on that line.
        let s = "// eslint-disable-next-line\nexport function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn eslint_disable_next_line_does_not_leak_past_its_target() {
        let s = "// eslint-disable-next-line @typescript-eslint/no-empty-function\n\
                 export function noop() {}\n\
                 export function noop2() {}\n";
        let kept = apply_suppressions(
            vec![diag(2, "ts-no-empty-function"), diag(3, "ts-no-empty-function")],
            Path::new("t.ts"),
            s,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].line, 3);
    }

    #[test]
    fn eslint_disable_block_comment_form_is_honored() {
        let s = "/* eslint-disable-next-line @typescript-eslint/no-empty-function */\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert!(kept.is_empty());
    }

    #[test]
    fn file_scope_eslint_disable_directive_is_not_honored() {
        // The file-scope opener below starts a range that a matching enable
        // directive closes; comply reads no range, so honoring the opener
        // alone would silence the rule past the author's intent.
        let s = "/* eslint-disable @typescript-eslint/no-empty-function */\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn eslint_disable_inside_a_string_literal_is_ignored() {
        let s = "const banner = \"// eslint-disable-next-line @typescript-eslint/no-empty-function\";\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn eslint_disable_needs_no_justification() {
        // The mandatory-justification rule is comply's own convention; ESLint
        // directives must not be reported as malformed comply markers.
        let r = parse_ignores(
            Path::new("t.ts"),
            "// eslint-disable-next-line @typescript-eslint/no-empty-function\nnoop();\n",
        );
        assert!(r.bad_ignores.is_empty());
    }

    #[test]
    fn marker_above_a_line_with_a_trailing_directive_stops_on_that_line() {
        // The trailing directive does not turn its line into a comment line;
        // the marker above still targets the code it sits on.
        let s = "// comply-ignore: rule-a — A\n\
                 export function noop() {} // eslint-disable-line no-empty-function\n\
                 export function noop2() {}\n";
        let kept = apply_suppressions(
            vec![diag(2, "rule-a"), diag(3, "rule-a")],
            Path::new("t.ts"),
            s,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].line, 3);
    }

    #[test]
    fn marker_above_a_line_with_a_block_directive_and_code_stops_on_that_line() {
        // A block directive ends mid-line, so code after it keeps the line a
        // code line — the marker above must not forward past it.
        let s = "// comply-ignore: rule-a — A\n\
                 /* eslint-disable-next-line no-console */ export function noop() {}\n\
                 export function noop2() {}\n";
        let kept = apply_suppressions(
            vec![diag(2, "rule-a"), diag(3, "rule-a")],
            Path::new("t.ts"),
            s,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].line, 3);
    }

    #[test]
    fn eslint_directive_inside_a_doc_block_suppresses_nothing() {
        // A rule name quoted in an `@example` must not silence that rule on
        // the declaration the block documents.
        let s = "/**\n\
                 * Adapter for the legacy payload.\n\
                 *\n\
                 * @example\n\
                 * // eslint-disable-next-line @typescript-eslint/no-empty-function\n\
                 * export function noop() {}\n\
                 */\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(8, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn eslint_directive_inside_an_unpadded_block_is_honored() {
        // A block comment whose interior lines carry no `*` leaves no trace on
        // them, so the per-line scan reads the directive as an instruction.
        // Pinned because the state that would see it is what silences whole
        // files; see `is_block_comment_padding`.
        let s = concat!(
            "/*\n",
            "// eslint-disable-next-line @typescript-eslint/no-empty-function\n",
            "*/ export function noop() {}\n",
        );
        let kept = apply_suppressions(vec![diag(3, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert!(kept.is_empty());
    }

    #[test]
    fn a_comment_opener_inside_a_string_does_not_silence_the_rest_of_the_file() {
        // The scan reads one line at a time, so a `/*` written inside a
        // template literal is not a comment opener and must not stop the
        // markers below it from being read.
        let s = concat!(
            "export const TPL = `\n",
            "  /* pattern\n",
            "`;\n",
            "// comply-ignore: rule-a — deliberate\n",
            "export const value = 1;\n",
        );
        let kept = apply_suppressions(vec![diag(5, "rule-a")], Path::new("t.ts"), s);
        assert!(kept.is_empty(), "the marker below the template still applies, got {kept:?}");
    }

    #[test]
    fn marker_after_a_block_comment_close_keeps_its_own_line_and_column() {
        let s = concat!(
            "/* doc\n",
            "   more */ // comply-ignore: rule-a\n",
            "export const p = 1;\n",
        );
        let r = parse_ignores(Path::new("t.ts"), s);
        // The marker trails the closing delimiter, so it targets its own line.
        assert!(r.suppressions.get(&2).is_some_and(|rules| rules.contains("rule-a")));
        // The column counts from the start of the line, not from the delimiter.
        assert_eq!(r.bad_ignores.len(), 1);
        assert_eq!(r.bad_ignores[0].column, 12);
    }

    #[test]
    fn marker_above_a_line_with_a_trailing_comply_marker_stops_on_that_line() {
        // Same forwarding rule for comply's own marker: the trailing marker
        // does not turn its line into a comment line.
        let s = "// comply-ignore: rule-a — A\n\
                 export const p = 1; // comply-ignore: rule-b — B\n\
                 export const q = 2;\n";
        let kept = apply_suppressions(
            vec![diag(2, "rule-a"), diag(3, "rule-a")],
            Path::new("t.ts"),
            s,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].line, 3);
    }

    #[test]
    fn eslint_directive_named_in_prose_suppresses_nothing() {
        let s = "// never add eslint-disable-next-line @typescript-eslint/no-empty-function\n\
                 export function noop() {}\n";
        let kept = apply_suppressions(vec![diag(2, "ts-no-empty-function")], Path::new("t.ts"), s);
        assert_eq!(kept.len(), 1);
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
