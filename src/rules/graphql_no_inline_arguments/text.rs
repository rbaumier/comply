//! graphql-no-inline-arguments — flags string/number literals passed inline
//! to a field's argument list inside an operation. Booleans and enums (bare
//! identifiers) are allowed: they are usually feature flags / constants.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only check operation files — schema-side argument defaults are fine.
        if !looks_like_operation(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, raw) in ctx.source.lines().enumerate() {
            let line = strip_comment(raw);
            // Quick filter: must contain `(` and `:` and not be on a type def.
            if !line.contains('(') || !line.contains(':') {
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with("type ")
                || trimmed.starts_with("input ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("enum ")
            {
                continue;
            }
            if has_literal_argument(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "graphql-no-inline-arguments".into(),
                    message: "Inline literal argument — use a variable (e.g. `field(id: $id)`) so the operation is cacheable and parameterized.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

/// Heuristic: does this look like an operation document (queries/mutations)
/// rather than a schema (type/input/enum/interface)?
fn looks_like_operation(source: &str) -> bool {
    for raw in source.lines() {
        let line = strip_comment(raw).trim_start();
        if line.starts_with("query") || line.starts_with("mutation") || line.starts_with("subscription") {
            return true;
        }
    }
    false
}

/// Walk argument lists `(...)` looking for a value that is a string or number
/// literal. Returns true on first hit.
fn has_literal_argument(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            // Find matching close, scan inside.
            let mut depth = 1;
            let mut j = i + 1;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                j += 1;
            }
            if j > bytes.len() {
                break;
            }
            let inner = &line[i + 1..j.min(line.len())];
            if scan_args_for_literal(inner) {
                return true;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    false
}

/// Inside an argument list, split on commas at top-level and check each
/// `name: value` pair.
fn scan_args_for_literal(inner: &str) -> bool {
    for part in split_top_level_commas(inner) {
        let Some((_, value)) = part.split_once(':') else { continue };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let first = value.chars().next().unwrap();
        // String literal.
        if first == '"' {
            return true;
        }
        // Number literal (int or float, optionally signed).
        if first.is_ascii_digit() || (matches!(first, '-' | '+') && value.len() > 1 && value.as_bytes()[1].is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut in_string = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' if depth == 0 => in_string = !in_string,
            b'(' | b'[' | b'{' if !in_string => depth += 1,
            b')' | b']' | b'}' if !in_string => depth -= 1,
            b',' if depth == 0 && !in_string => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("op.graphql"), source))
    }

    #[test]
    fn flags_string_literal_argument() {
        let src = "query GetUser { user(id: \"123\") { name } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_number_literal_argument() {
        let src = "query Posts { posts(limit: 10) { title } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_variable_argument() {
        let src = "query GetUser($id: ID!) { user(id: $id) { name } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_enum_argument() {
        let src = "query Posts { posts(order: ASC) { title } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_boolean_argument() {
        let src = "query Posts { posts(active: true) { title } }";
        assert!(run(src).is_empty());
    }
}
