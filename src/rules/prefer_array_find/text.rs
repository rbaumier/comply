use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Walk past a balanced parenthesised group starting at `bytes[start]` == `(`.
/// Returns the index *after* the closing `)`, or `None` if unbalanced.
fn skip_parens(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1u32;
    let mut i = start + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    if depth == 0 { Some(i) } else { None }
}

/// Detect `.filter(…)[0]`, `.filter(…).at(0)`, and `.filter(…).shift()`.
fn has_filter_first_element(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".filter(") {
        let open_paren = start + pos + 7; // index of '('
        if let Some(after_paren) = skip_parens(bytes, open_paren) {
            let rest = line[after_paren..].trim_start();
            // .filter(…)[0]
            if rest.starts_with("[0]") {
                return true;
            }
            // .filter(…).at(0)
            if rest.starts_with(".at(0)") || rest.starts_with("?.at(0)") {
                return true;
            }
            // .filter(…).shift()
            if rest.starts_with(".shift(") || rest.starts_with("?.shift(") {
                return true;
            }
        }
        start = open_paren + 1;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_filter_first_element(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-array-find".into(),
                    message: "Prefer `.find(…)` over `.filter(…)[0]` — `.find()` short-circuits on the first match.".into(),
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
    fn flags_filter_zero_index() {
        assert_eq!(run("const x = arr.filter(fn)[0];").len(), 1);
    }

    #[test]
    fn flags_filter_at_zero() {
        assert_eq!(run("const x = arr.filter(fn).at(0);").len(), 1);
    }

    #[test]
    fn flags_filter_shift() {
        assert_eq!(run("const x = arr.filter(fn).shift();").len(), 1);
    }

    #[test]
    fn flags_optional_filter_at() {
        assert_eq!(run("const x = arr.filter(fn)?.at(0);").len(), 1);
    }

    #[test]
    fn allows_find() {
        assert!(run("const x = arr.find(fn);").is_empty());
    }

    #[test]
    fn allows_filter_alone() {
        assert!(run("const x = arr.filter(fn);").is_empty());
    }

    #[test]
    fn allows_filter_non_zero_index() {
        assert!(run("const x = arr.filter(fn)[1];").is_empty());
    }
}
