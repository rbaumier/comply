//! comply-ignore parser — scans source for suppression comments + filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>` (em-dash or ` -- `).
//! The marker must be the first non-whitespace content on the line —
//! otherwise a string literal containing `"// comply-ignore: ..."` would
//! register a phantom suppression. Justification is mandatory; missing →
//! emit `comply-ignore-missing-justification` diagnostic.

mod payload;

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const MARKER: &str = "// comply-ignore:";

/// Result of parsing comply-ignore comments in a source file.
pub struct IgnoreResult {
    /// Map from suppressed line number → set of suppressed rule ids on that line.
    /// Keyed this way (instead of HashSet<(line, String)>) so the lookup in
    /// `apply_suppressions` doesn't have to clone the rule_id on every check.
    pub suppressions: HashMap<usize, HashSet<String>>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions: HashMap<usize, HashSet<String>> = HashMap::new();
    let mut bad_ignores = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim_start();
        if !trimmed.starts_with(MARKER) {
            continue;
        }
        let parsed = payload::parse(&trimmed[MARKER.len()..]);
        if parsed.rule_id.is_empty() {
            continue;
        }
        if parsed.justification.is_empty() {
            let leading_ws = line.len() - trimmed.len();
            let char_column = line[..leading_ws].chars().count();
            bad_ignores.push(make_bad_ignore_diagnostic(
                path,
                line_num,
                char_column,
                &parsed.rule_id,
            ));
        }
        // Suppress the line below even if justification was missing — the user
        // intent is clear and the bad-ignore diagnostic already nags about it.
        suppressions
            .entry(line_num + 1)
            .or_default()
            .insert(parsed.rule_id);
    }

    IgnoreResult {
        suppressions,
        bad_ignores,
    }
}

/// Construct a diagnostic for a comply-ignore comment missing its justification.
fn make_bad_ignore_diagnostic(
    path: &Path,
    line: usize,
    char_column: usize,
    rule_id: &str,
) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line,
        column: char_column + 1, // 1-indexed for editor consumption
        rule_id: "comply-ignore-missing-justification".into(),
        message: format!(
            "comply-ignore without justification — explain why this exception \
             is needed: `// comply-ignore: {rule_id} — <reason>`"
        ),
        severity: Severity::Error,
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
        // Look up the line in the map first; only the inner set lookup needs
        // to compare against the rule_id, and HashSet<String>::contains takes
        // &str so we don't allocate.
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
/// Iterates over every discovered file (not just files with diagnostics) so
/// malformed `comply-ignore` comments in clean files are still flagged.
/// Files that can't be read pass through unchanged + warn on stderr.
pub fn apply_to_all(
    diagnostics: Vec<Diagnostic>,
    discovered: &[crate::files::SourceFile],
) -> Vec<Diagnostic> {
    let mut by_file: HashMap<std::path::PathBuf, Vec<Diagnostic>> =
        HashMap::with_capacity(diagnostics.len());
    for d in diagnostics {
        by_file.entry(d.path.clone()).or_default().push(d);
    }

    let mut result = Vec::with_capacity(by_file.values().map(Vec::len).sum::<usize>());
    for file in discovered {
        let file_diags = by_file.remove(&file.path).unwrap_or_default();
        match std::fs::read_to_string(&file.path) {
            Ok(src) => result.extend(apply_suppressions(file_diags, &file.path, &src)),
            Err(e) => {
                eprintln!("comply: skipping ignore-scan for {}: {e}", file.path.display());
                result.extend(file_diags);
            }
        }
    }
    // Files not in `discovered` (e.g. oxlint canonicalized differently) pass through.
    for (_, file_diags) in by_file {
        result.extend(file_diags);
    }
    result
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
    fn parse_extracts_suppression() {
        let r = parse_ignores(Path::new("t.ts"), "// comply-ignore: no-throw — ok\nx;");
        assert!(
            r.suppressions
                .get(&2)
                .is_some_and(|s| s.contains("no-throw"))
        );
        assert!(r.bad_ignores.is_empty());
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
        let filtered = apply_suppressions(vec![diag(2, "no-other")], Path::new("t.ts"), s);
        assert_eq!(filtered.len(), 1);
    }
}
