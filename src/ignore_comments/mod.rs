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
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Result of parsing comply-ignore comments in a source file.
#[derive(Debug)]
pub struct IgnoreResult {
    /// Map: line number → set of rule ids suppressed on that line. Keyed
    /// this way (instead of HashSet<(line, String)>) so the lookup in
    /// `apply_suppressions` doesn't have to clone the rule_id per check.
    pub suppressions: HashMap<usize, HashSet<String>>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut bad_ignores = Vec::new();

    // Strip leading UTF-8 BOM — `is_whitespace` doesn't include U+FEFF, so
    // a line-1 ignore in a BOM-prefixed file would never apply otherwise.
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);

    for (idx, raw_line) in source.lines().enumerate() {
        if let Some(parsed) = line::parse(path, raw_line, idx + 1) {
            if let Some(d) = parsed.bad_ignore {
                bad_ignores.push(d);
            }
            suppressions
                .entry(parsed.target_line)
                .or_default()
                .insert(parsed.rule_id);
        }
    }

    IgnoreResult {
        suppressions,
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
        let is_suppressed = ignore_result
            .suppressions
            .get(&diag.line)
            .is_some_and(|rules| rules.contains(diag.rule_id.as_str()));
        if !is_suppressed {
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
    let mut by_file: HashMap<std::path::PathBuf, Vec<Diagnostic>> =
        HashMap::with_capacity(diagnostics.len());
    for d in diagnostics {
        let key = canonical_key(&d.path);
        by_file.entry(key).or_default().push(d);
    }

    let mut result = Vec::with_capacity(by_file.values().map(Vec::len).sum::<usize>());
    for file in discovered {
        let key = canonical_key(&file.path);
        let file_diags = by_file.remove(&key).unwrap_or_default();
        match std::fs::read_to_string(&file.path) {
            Ok(src) => result.extend(apply_suppressions(file_diags, &file.path, &src)),
            Err(e) => {
                eprintln!(
                    "comply: skipping ignore-scan for {}: {e}",
                    file.path.display()
                );
                result.extend(file_diags);
            }
        }
    }
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

    fn diag(line: usize, rule_id: &str) -> Diagnostic {
        Diagnostic {
            path: Path::new("t.ts").to_path_buf(),
            line,
            column: 1,
            rule_id: rule_id.into(),
            message: "test".into(),
            severity: Severity::Error,
        }
    }

    #[test]
    fn parse_extracts_above_line_suppression() {
        let r = parse_ignores(Path::new("t.ts"), "// comply-ignore: no-throw — ok\nx;");
        assert!(r.suppressions.get(&2).is_some_and(|s| s.contains("no-throw")));
        assert!(r.bad_ignores.is_empty());
    }

    #[test]
    fn parse_extracts_trailing_suppression() {
        let r = parse_ignores(
            Path::new("t.ts"),
            "throw err; // comply-ignore: no-throw — legacy\n",
        );
        assert!(r.suppressions.get(&1).is_some_and(|s| s.contains("no-throw")));
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
}
