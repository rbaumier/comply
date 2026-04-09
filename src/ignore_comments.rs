//! comply-ignore parser — scans source for suppression comments and filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>`
//! Or:     `// comply-ignore: <rule-id> -- <justification>` (ASCII fallback)
//!
//! The justification after the separator is mandatory — if missing, comply emits
//! its own diagnostic so suppressions are always documented. This is the
//! load-bearing rule of the whole mechanism: a silently-suppressed warning is
//! tech debt no one ever pays back.
//!
//! How it works:
//! 1. Walk each line looking for the literal `// comply-ignore:` marker.
//! 2. Strip the marker, split the remainder on the first `—` or ` -- `.
//! 3. Trim both halves: left = rule-id, right = justification.
//! 4. Empty rule-id → skip silently. Empty justification → emit a diagnostic.
//! 5. Build a `(line+1, rule-id)` set so the next-line diagnostic gets removed
//!    by `apply_suppressions`.
//!
//! Limitation: the marker is matched textually, so a string literal containing
//! `"// comply-ignore: ..."` would register a phantom suppression. Acceptable
//! for v1 — moving the scan into the AST is the v2 fix.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;
use std::path::Path;

const MARKER: &str = "// comply-ignore:";
const EM_DASH: char = '—';
/// Padded ASCII fallback — the spaces around `--` prevent collision with the
/// hyphens inside rule ids like `no-nested-ternary`.
const ASCII_SEP: &str = " -- ";

/// Result of parsing comply-ignore comments in a source file.
pub struct IgnoreResult {
    /// `(line_to_suppress, rule_id)` pairs — the line below the comment.
    pub suppressions: HashSet<(usize, String)>,
    /// Diagnostics for malformed comply-ignore comments (missing justification).
    pub bad_ignores: Vec<Diagnostic>,
}

/// One parsed comply-ignore comment after splitting on the separator.
struct ParsedIgnore {
    rule_id: String,
    /// Empty if no justification was provided.
    justification: String,
}

/// Split a `// comply-ignore:` payload into `(rule_id, justification)`.
/// Both are trimmed; either may be empty if not present.
fn parse_ignore_payload(payload: &str) -> ParsedIgnore {
    let trimmed = payload.trim();

    // Try em-dash first, then padded ASCII `--`.
    let split = trimmed
        .split_once(EM_DASH)
        .or_else(|| trimmed.split_once(ASCII_SEP));

    let (rule_part, justification_part) = match split {
        Some((left, right)) => (left, right),
        None => (trimmed, ""),
    };

    ParsedIgnore {
        rule_id: rule_part.trim().to_string(),
        justification: justification_part.trim().to_string(),
    }
}

/// Parse all comply-ignore comments in source text.
pub fn parse_ignores(path: &Path, source: &str) -> IgnoreResult {
    let mut suppressions = HashSet::with_capacity(8);
    let mut bad_ignores = Vec::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1; // 1-indexed.

        let Some(marker_start) = line.find(MARKER) else {
            continue;
        };

        let payload = &line[marker_start + MARKER.len()..];
        let parsed = parse_ignore_payload(payload);

        if parsed.rule_id.is_empty() {
            continue; // Empty rule-id — silently skip, nothing to suppress.
        }

        if parsed.justification.is_empty() {
            bad_ignores.push(Diagnostic {
                path: path.to_path_buf(),
                line: line_num,
                column: marker_start + 1,
                rule_id: "comply-ignore-missing-justification".into(),
                message: format!(
                    "comply-ignore without justification — explain why this exception \
                     is needed: `// comply-ignore: {} — <reason>`",
                    parsed.rule_id
                ),
                severity: Severity::Error,
            });
        }

        // Suppress the rule on the line BELOW the comment, even if the
        // justification was missing — the user's intent was to suppress.
        suppressions.insert((line_num + 1, parsed.rule_id));
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
    let mut result: Vec<Diagnostic> =
        Vec::with_capacity(diagnostics.len() + ignore_result.bad_ignores.len());

    for d in diagnostics {
        let key = (d.line, d.rule_id.clone());
        if !ignore_result.suppressions.contains(&key) {
            result.push(d);
        }
    }

    result.extend(ignore_result.bad_ignores);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    fn make_diag(line: usize, rule_id: &str) -> Diagnostic {
        Diagnostic {
            path: Path::new("test.ts").to_path_buf(),
            line,
            column: 1,
            rule_id: rule_id.into(),
            message: "test".into(),
            severity: Severity::Error,
        }
    }

    #[test]
    fn parse_ignores_with_em_dash_justification() {
        let source = "// comply-ignore: no-throw — legacy code\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(result.suppressions.contains(&(2, "no-throw".into())));
        assert!(result.bad_ignores.is_empty());
    }

    #[test]
    fn parse_ignores_with_ascii_dash_justification() {
        let source = "// comply-ignore: no-throw -- legacy code\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(result.suppressions.contains(&(2, "no-throw".into())));
        assert!(result.bad_ignores.is_empty());
    }

    #[test]
    fn parse_ignores_flags_missing_justification() {
        let source = "// comply-ignore: no-throw\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert_eq!(result.bad_ignores.len(), 1);
        assert_eq!(
            result.bad_ignores[0].rule_id,
            "comply-ignore-missing-justification"
        );
    }

    #[test]
    fn parse_ignores_accepts_numeric_justification() {
        // Regression: previous parser only accepted alphabetic justifications.
        let source = "// comply-ignore: no-throw — see #4521\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(
            result.bad_ignores.is_empty(),
            "numeric/symbolic justification must be accepted"
        );
    }

    #[test]
    fn parse_ignores_accepts_punctuation_only_justification() {
        // Regression: poor justification but still non-empty — should pass.
        let source = "// comply-ignore: no-throw — !!!\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(result.bad_ignores.is_empty());
    }

    #[test]
    fn parse_ignores_handles_rule_with_hyphens() {
        // Regression: " -- " separator must not collide with hyphens inside rule ids.
        let source = "// comply-ignore: no-nested-ternary -- legacy form\nlet x = a?b?1:2:3;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(
            result
                .suppressions
                .contains(&(2, "no-nested-ternary".into())),
            "rule id with hyphens must round-trip cleanly"
        );
    }

    #[test]
    fn apply_suppressions_removes_matching_diagnostic() {
        let source = "// comply-ignore: no-throw — needed\nthrow err;";
        let diags = vec![make_diag(2, "no-throw")];
        let filtered = apply_suppressions(diags, Path::new("test.ts"), source);
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn apply_suppressions_keeps_unrelated_diagnostic() {
        let source = "// comply-ignore: no-throw — needed\nlet x = 5;";
        let diags = vec![make_diag(2, "no-nested-ternary")];
        let filtered = apply_suppressions(diags, Path::new("test.ts"), source);
        assert_eq!(filtered.len(), 1);
    }
}
