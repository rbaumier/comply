use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Heuristic names that suggest an array/iterable on the right side of `in`.
const ITERABLE_HINTS: &[&str] = &["arr", "list", "items", "elements", "array", "values", "entries", "results", "rows", "records"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(col) = has_for_in_iterable(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "no-for-in-iterable".into(),
                    message: "`for...in` on an array/iterable — use `for...of` instead.".into(),
                    severity: Severity::Error,
                });
            }
        }
        diagnostics
    }
}

/// Detect `for (... in ...)` where the right side looks like an array/iterable.
fn has_for_in_iterable(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let mut start = 0;
    while let Some(for_pos) = line[start..].find("for") {
        let abs = start + for_pos;
        // Ensure `for` is not part of a longer identifier
        if abs > 0 && line.as_bytes()[abs - 1].is_ascii_alphanumeric() {
            start = abs + 3;
            continue;
        }
        let after_for = abs + 3;
        if after_for < line.len() && line.as_bytes()[after_for].is_ascii_alphanumeric() {
            start = abs + 3;
            continue;
        }
        let rest = &line[after_for..];
        let rest_trimmed = rest.trim_start();
        if !rest_trimmed.starts_with('(') {
            start = abs + 3;
            continue;
        }
        // Extract contents inside the parens (simple — find matching `)`)
        let paren_start = after_for + (rest.len() - rest_trimmed.len()) + 1;
        if let Some(paren_end) = find_matching_paren(line, paren_start) {
            let inside = &line[paren_start..paren_end];
            // Must contain ` in ` (with spaces) — a for-in loop
            if let Some(in_pos) = find_in_keyword(inside) {
                let rhs = inside[in_pos + 3..].trim();
                if looks_like_iterable(rhs) {
                    return Some(abs);
                }
            }
        }
        start = abs + 3;
    }
    None
}

/// Find the `in` keyword surrounded by spaces (not `include`, `index`, etc.).
fn find_in_keyword(s: &str) -> Option<usize> {
    s.find(" in ")
}

/// Find the matching `)` given that `start` points right after `(`.
fn find_matching_paren(line: &str, start: usize) -> Option<usize> {
    let mut depth = 1i32;
    for (i, ch) in line[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Heuristic: the right-hand side of `in` looks like an array/iterable.
fn looks_like_iterable(rhs: &str) -> bool {
    // Direct array literal: `[...]`
    if rhs.starts_with('[') {
        return true;
    }
    let rhs_lower = rhs.to_ascii_lowercase();
    for hint in ITERABLE_HINTS {
        if rhs_lower.contains(hint) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_for_in_with_array_name() {
        assert_eq!(run("for (const x in myArray) {}").len(), 1);
    }

    #[test]
    fn flags_for_in_with_list_name() {
        assert_eq!(run("for (let key in itemsList) {}").len(), 1);
    }

    #[test]
    fn flags_for_in_with_literal_array() {
        assert_eq!(run("for (const i in [1, 2, 3]) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_with_object() {
        assert!(run("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_for_of() {
        assert!(run("for (const x of myArray) {}").is_empty());
    }
}
