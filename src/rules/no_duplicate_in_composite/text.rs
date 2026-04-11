use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

/// Split a composite type expression by a separator (`|` or `&`) and check for duplicates.
fn has_duplicate_members(segment: &str, sep: char) -> bool {
    let parts: Vec<&str> = segment
        .split(sep)
        .map(|s| s.trim().trim_end_matches(';').trim_end_matches(',').trim())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    let mut seen = HashSet::new();
    for part in &parts {
        if !part.is_empty() && !seen.insert(*part) {
            return true;
        }
    }
    false
}

/// Extract the type expression from a line (after `=` for type aliases, after `:` for annotations).
fn type_expr(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("type ")
        && let Some(eq_pos) = trimmed.find('=') {
            return Some(trimmed[eq_pos + 1..].to_string());
        }
    if let Some(colon_pos) = trimmed.find(':') {
        let after_colon = &trimmed[colon_pos + 1..];
        // For annotations like `x: A | B)`, cut at the closing `)` that
        // ends the parameter list.
        let end = after_colon
            .find(')')
            .unwrap_or(after_colon.len());
        let cleaned = &after_colon[..end];
        return Some(cleaned.to_string());
    }
    None
}

fn has_duplicate_composite(line: &str) -> bool {
    if let Some(expr) = type_expr(line) {
        if has_duplicate_members(&expr, '|') {
            return true;
        }
        if has_duplicate_members(&expr, '&') {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_duplicate_composite(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-duplicate-in-composite".into(),
                    message: "Duplicate type in composite — remove the repeated member.".into(),
                    severity: Severity::Warning,
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
    fn flags_duplicate_in_union() {
        assert_eq!(run("type X = string | string;").len(), 1);
    }

    #[test]
    fn flags_duplicate_in_intersection() {
        assert_eq!(run("type X = A & A;").len(), 1);
    }

    #[test]
    fn flags_duplicate_in_annotation() {
        assert_eq!(run("function foo(x: number | number) {}").len(), 1);
    }

    #[test]
    fn allows_unique_members() {
        assert!(run("type X = string | number;").is_empty());
    }

    #[test]
    fn allows_single_type() {
        assert!(run("type X = string;").is_empty());
    }
}
