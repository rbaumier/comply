use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.indexOf(...)` compared with `> 0` or `< 1`.
fn has_indexof_positive_compare(line: &str) -> bool {
    // Find all occurrences of `.indexOf(`
    let mut start = 0;
    while let Some(pos) = line[start..].find(".indexOf(") {
        let abs = start + pos + 9; // skip past ".indexOf("
        // Walk forward to find the matching closing paren (handle nesting).
        let mut depth = 1u32;
        let mut i = abs;
        let bytes = line.as_bytes();
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        if depth == 0 {
            // `i` is right after the closing `)`.
            let after = line[i..].trim_start();
            if after.starts_with("> 0") || after.starts_with("< 1") {
                return true;
            }
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_indexof_positive_compare(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "index-of-compare-to-positive".into(),
                    message: "`.indexOf(…) > 0` misses index 0 — use `>= 0` or `!== -1`.".into(),
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
    fn flags_indexof_gt_zero() {
        assert_eq!(run("if (arr.indexOf(x) > 0) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_lt_one() {
        assert_eq!(run("if (str.indexOf('a') < 1) {}").len(), 1);
    }

    #[test]
    fn allows_indexof_gte_zero() {
        assert!(run("if (arr.indexOf(x) >= 0) {}").is_empty());
    }

    #[test]
    fn allows_indexof_neq_minus_one() {
        assert!(run("if (arr.indexOf(x) !== -1) {}").is_empty());
    }
}
