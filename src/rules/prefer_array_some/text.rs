use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Walk past a balanced parenthesised group starting at `bytes[start]` == `(`.
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

/// Detect `.filter(…).length > 0`, `.filter(…).length !== 0`,
/// `.filter(…).length != 0`, and `.filter(…).length >= 1`.
fn has_filter_length_check(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut start = 0;
    while let Some(pos) = line[start..].find(".filter(") {
        let open_paren = start + pos + 7;
        if let Some(after_paren) = skip_parens(bytes, open_paren) {
            let rest = line[after_paren..].trim_start();
            if let Some(after_length) = rest.strip_prefix(".length") {
                let cmp = after_length.trim_start();
                if cmp.starts_with("> 0")
                    || cmp.starts_with("!== 0")
                    || cmp.starts_with("!= 0")
                    || cmp.starts_with(">= 1")
                {
                    return true;
                }
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
            if has_filter_length_check(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-array-some".into(),
                    message: "Prefer `.some(…)` over `.filter(…).length` check — `.some()` short-circuits.".into(),
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
    fn flags_filter_length_gt_zero() {
        assert_eq!(run("if (arr.filter(fn).length > 0) {}").len(), 1);
    }

    #[test]
    fn flags_filter_length_not_equal_zero() {
        assert_eq!(run("if (arr.filter(fn).length !== 0) {}").len(), 1);
    }

    #[test]
    fn flags_filter_length_loose_not_equal() {
        assert_eq!(run("if (arr.filter(fn).length != 0) {}").len(), 1);
    }

    #[test]
    fn flags_filter_length_gte_one() {
        assert_eq!(run("if (arr.filter(fn).length >= 1) {}").len(), 1);
    }

    #[test]
    fn allows_some() {
        assert!(run("if (arr.some(fn)) {}").is_empty());
    }

    #[test]
    fn allows_filter_length_alone() {
        assert!(run("const n = arr.filter(fn).length;").is_empty());
    }
}
