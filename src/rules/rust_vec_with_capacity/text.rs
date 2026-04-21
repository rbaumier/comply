//! rust-vec-with-capacity backend.
//!
//! Walks line-by-line looking for `let mut X = Vec::new();` and then
//! scans the next ~20 lines for a `for … in …` loop that calls
//! `X.push(…)`. When both are present, the Vec's final length is
//! knowable up front and `Vec::with_capacity(n)` avoids the
//! log2(n) reallocation chain from doubling.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if !t.contains("Vec::new()") || !t.starts_with("let mut ") { continue; }
            let after = &t["let mut ".len()..];
            let var = match after.split_whitespace().next() {
                Some(v) => v.trim_end_matches(':').trim_end_matches('='),
                None => continue,
            };
            if var.is_empty() { continue; }
            let push_pattern = format!("{var}.push(");
            let look_ahead = &lines[i + 1..std::cmp::min(i + 20, lines.len())];
            let has_for = look_ahead.iter().any(|l| l.contains("for ") && l.contains(" in "));
            let has_push = look_ahead.iter().any(|l| l.contains(&push_pattern));
            if has_for && has_push {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Use `Vec::with_capacity(...)` instead of `Vec::new()` when `{var}` is populated in a for-loop."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_vec_new_then_push_in_for() {
        let src = "let mut result = Vec::new();\nfor item in items {\n    result.push(item);\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_capacity() {
        let src = "let mut result = Vec::with_capacity(items.len());\nfor item in items {\n    result.push(item);\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vec_new_no_for() {
        assert!(run("let mut v = Vec::new();\nv.push(1);").is_empty());
    }
}
