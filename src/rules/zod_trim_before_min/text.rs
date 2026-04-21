//! zod-trim-before-min backend — flag lines that chain `z.string()` with
//! `.min(…)` but omit `.trim()`. Whitespace-only user input otherwise
//! satisfies `.min(1)` and sneaks past validation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("z.string()") || !line.contains(".min(") {
                continue;
            }
            if line.trim().starts_with("//") {
                continue;
            }
            if line.contains(".trim()") {
                continue;
            }
            if let Some(pos) = line.find("z.string()") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: pos + 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings."
                            .into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_min_without_trim() {
        assert_eq!(run("z.string().min(1)").len(), 1);
    }

    #[test]
    fn allows_trim_before_min() {
        assert!(run("z.string().trim().min(1)").is_empty());
    }
}
