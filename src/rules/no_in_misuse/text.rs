use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Heuristic names that suggest an array.
const ARRAY_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "values", "entries", "rows", "results",
];

/// Check whether `rhs` looks like an array name or literal.
fn looks_like_array(rhs: &str) -> bool {
    let lower = rhs.to_ascii_lowercase();

    // Array literal: `[...]`
    if rhs.starts_with('[') {
        return true;
    }

    // Name contains a common array hint.
    ARRAY_HINTS.iter().any(|hint| lower.contains(hint))
}

/// Find `<expr> in <rhs>` patterns where `rhs` looks like an array.
fn detect_in_misuse(line: &str) -> bool {
    let trimmed = line.trim();

    // Skip comments.
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return false;
    }

    // Skip `for ... in` loops — those are valid usage.
    if trimmed.starts_with("for ") || trimmed.starts_with("for(") {
        return false;
    }

    // Look for ` in ` token (surrounded by whitespace to avoid matching `include`, `index`, etc.)
    let mut start = 0;
    while let Some(pos) = line[start..].find(" in ") {
        let abs = start + pos;
        let rhs = line[abs + 4..].trim();

        // Take the first token from RHS.
        let rhs_token_end = rhs
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '$' && c != '[' && c != ']')
            .unwrap_or(rhs.len());
        let rhs_token = &rhs[..rhs_token_end];

        if !rhs_token.is_empty() && looks_like_array(rhs_token) {
            return true;
        }

        start = abs + 4;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if detect_in_misuse(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-in-misuse".into(),
                    message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_in_on_array_name() {
        assert_eq!(run("if (\"x\" in myItems) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_arr_suffix() {
        assert_eq!(run("if (val in userList) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_loop() {
        assert!(run("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_in_on_object() {
        assert!(run("if (\"name\" in config) {}").is_empty());
    }
}
