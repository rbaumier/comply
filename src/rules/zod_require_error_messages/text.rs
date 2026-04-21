//! zod-require-error-messages backend — flag single-argument `.refine(...)`
//! calls. We look at the trimmed line, confirm it is not a comment, and
//! walk the bytes after `.refine(` to count top-level commas. Zero
//! top-level commas means no options object was passed, so any thrown
//! error will carry Zod's generic fallback message.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains(".refine(") || t.starts_with("//") {
                continue;
            }
            if t.contains("message") || t.contains("{ message") {
                continue;
            }
            if t.contains(".refine(") && (t.ends_with(")") || t.ends_with(");") || t.ends_with("),"))
            {
                let after_refine = t.split(".refine(").nth(1).unwrap_or("");
                let mut depth = 0usize;
                let mut comma_count = 0;
                for c in after_refine.chars() {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => {
                            depth = depth.saturating_sub(1);
                        }
                        ',' if depth == 0 => comma_count += 1,
                        _ => {}
                    }
                }
                if comma_count == 0 {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(".refine(").unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message:
                            "Add `{ message: '...' }` to `.refine()` — bare refine produces no helpful error message."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_single_arg_refine() {
        assert_eq!(run("z.string().refine(val => val.includes('@'))").len(), 1);
    }

    #[test]
    fn allows_refine_with_message() {
        assert!(run(
            "z.string().refine(val => val.includes('@'), { message: 'Must be email' })"
        )
        .is_empty());
    }
}
