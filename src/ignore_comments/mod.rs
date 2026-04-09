//! comply-ignore parser — scans source for suppression comments and filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>`
//! Or:     `// comply-ignore: <rule-id> -- <justification>` (ASCII fallback)
//!
//! How it works:
//! 1. Walk each line looking for the literal `// comply-ignore:` marker.
//! 2. Strip the marker, hand the remainder to `payload::parse`.
//! 3. Empty rule-id → skip silently. Empty justification → emit a diagnostic.
//! 4. Build a `(line+1, rule-id)` set so the next-line diagnostic gets removed
//!    by `apply_suppressions`.
//!
//! Limitation: the marker is matched textually, so a string literal containing
//! `"// comply-ignore: ..."` would register a phantom suppression. Acceptable
//! for v1 — moving the scan into the AST is the v2 fix.

mod payload;

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;
use std::path::Path;

const MARKER: &str = "// comply-ignore:";

/// Result of parsing comply-ignore comments in a source file.
pub struct IgnoreResult {
    /// `(line_to_suppress, rule_id)` pairs — the line below the comment.
    pub suppressions: HashSet<(usize, String)>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions = HashSet::new();
    let mut bad_ignores = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1; // 1-indexed.
        let Some(marker_start) = line.find(MARKER) else {
            continue;
        };
        let parsed = payload::parse(&line[marker_start + MARKER.len()..]);
        if parsed.rule_id.is_empty() {
            continue;
        }
        if parsed.justification.is_empty() {
            bad_ignores.push(make_bad_ignore_diagnostic(
                path,
                line_num,
                marker_start,
                &parsed.rule_id,
            ));
        }
        // Suppress the line below even if justification was missing — the
        // user clearly intended to suppress, and the bad-ignore diagnostic
        // already nags them about the missing reason.
        suppressions.insert((line_num + 1, parsed.rule_id));
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
    column: usize,
    rule_id: &str,
) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line,
        // NOTE: byte offset, not char offset — wrong for lines with multibyte
        // chars before the marker. v2: convert via line[..byte_offset].chars().count().
        column: column + 1,
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

    for d in diagnostics {
        if !ignore_result.suppressions.contains(&(d.line, d.rule_id.clone())) {
            result.push(d);
        }
    }
    result.extend(ignore_result.bad_ignores);
    result
}

/// Apply comply-ignore suppressions across every discovered file.
///
/// Iterates over every discovered file (not just files with diagnostics) so
/// that malformed `comply-ignore` comments in clean files are still flagged.
/// Files that can't be read are reported on stderr and their diagnostics pass
/// through unchanged — a missing source shouldn't crash the whole report.
pub fn apply_to_all(
    diagnostics: Vec<Diagnostic>,
    discovered: &[crate::files::SourceFile],
) -> Vec<Diagnostic> {
    let mut by_file = group_by_path(diagnostics);
    let total: usize = by_file.values().map(Vec::len).sum();
    let mut result = Vec::with_capacity(total);

    for file in discovered {
        let file_diags = by_file.remove(&file.path).unwrap_or_default();
        match std::fs::read_to_string(&file.path) {
            Ok(source) => result.extend(apply_suppressions(file_diags, &file.path, &source)),
            Err(e) => {
                eprintln!(
                    "comply: skipping ignore-comment scan for {}: {e}",
                    file.path.display()
                );
                result.extend(file_diags);
            }
        }
    }

    // Diagnostics for files NOT in the discovered list (e.g. paths normalized
    // differently by oxlint) — keep them as-is, no suppression possible.
    for (_, file_diags) in by_file {
        result.extend(file_diags);
    }

    result
}

/// Group a flat diagnostic list by source path.
fn group_by_path(
    diagnostics: Vec<Diagnostic>,
) -> std::collections::HashMap<std::path::PathBuf, Vec<Diagnostic>> {
    let mut by_file: std::collections::HashMap<std::path::PathBuf, Vec<Diagnostic>> =
        std::collections::HashMap::with_capacity(diagnostics.len());
    for d in diagnostics {
        by_file.entry(d.path.clone()).or_default().push(d);
    }
    by_file
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
        assert!(r.suppressions.contains(&(2, "no-throw".into())));
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
