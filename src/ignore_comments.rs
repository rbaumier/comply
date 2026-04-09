//! comply-ignore parser — scans source for suppression comments and filters diagnostics.
//!
//! Format: `// comply-ignore: <rule-id> — <justification>`
//! The justification after the em-dash is mandatory — if missing, comply emits
//! its own diagnostic so suppressions are always documented.
//!
//! How it works:
//! 1. Scan each line for `// comply-ignore:` comments.
//! 2. Extract the rule-id and check for the `—` justification separator.
//! 3. Build a set of (line_number + 1, rule_id) pairs that are suppressed.
//! 4. Filter diagnostics: remove any whose (line, rule_id) is in the set.
//! 5. Add diagnostics for any comply-ignore without justification.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;
use std::path::Path;

/// Result of parsing comply-ignore comments in a source file.
pub struct IgnoreResult {
    /// (next_line, rule_id) pairs to suppress.
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

        let Some(comment_start) = line.find("// comply-ignore:") else {
            continue;
        };

        let after_prefix = &line[comment_start + "// comply-ignore:".len()..];
        let trimmed = after_prefix.trim();

        // Check for em-dash separator (— or --) for justification.
        let (rule_part, has_justification) =
            if let Some(pos) = trimmed.find('—').or_else(|| trimmed.find("--")) {
                let justification = trimmed[pos + trimmed[pos..].find(char::is_alphabetic).unwrap_or(trimmed.len() - pos)..].trim();
                (trimmed[..pos].trim(), !justification.is_empty())
            } else {
                (trimmed, false)
            };

        let rule_id = rule_part.trim().to_string();

        if rule_id.is_empty() {
            continue;
        }

        if !has_justification {
            bad_ignores.push(Diagnostic {
                path: path.to_path_buf(),
                line: line_num,
                column: comment_start + 1,
                rule_id: "comply-ignore-missing-justification".into(),
                message: format!(
                    "comply-ignore without justification — explain why this exception \
                     is needed: `// comply-ignore: {rule_id} — <reason>`"
                ),
                severity: Severity::Error,
            });
        }

        // Suppress the rule on the NEXT line.
        suppressions.insert((line_num + 1, rule_id));
    }

    IgnoreResult {
        suppressions,
        bad_ignores,
    }
}

/// Filter diagnostics by removing suppressed ones. Returns filtered list + bad-ignore diagnostics.
pub fn apply_suppressions(
    diagnostics: Vec<Diagnostic>,
    path: &Path,
    source: &str,
) -> Vec<Diagnostic> {
    let ignore_result = parse_ignores(path, source);
    let mut result: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|d| !ignore_result.suppressions.contains(&(d.line, d.rule_id.clone())))
        .collect();

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
    fn parse_ignores_extracts_rule_with_justification() {
        let source = "// comply-ignore: no-throw — legacy code\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert!(result.suppressions.contains(&(2, "no-throw".into())));
        assert!(result.bad_ignores.is_empty());
    }

    #[test]
    fn parse_ignores_flags_missing_justification() {
        let source = "// comply-ignore: no-throw\nthrow err;";
        let result = parse_ignores(Path::new("test.ts"), source);
        assert_eq!(result.bad_ignores.len(), 1);
        assert_eq!(result.bad_ignores[0].rule_id, "comply-ignore-missing-justification");
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
