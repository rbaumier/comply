//! zod-string-min-1-required backend — flag `z.string()` occurrences that
//! are not followed (on the same line) by any length, format, or chain
//! method that would either reject empty strings or make the field
//! optional/nullable. The heuristic is line-local to keep the scan cheap
//! and predictable.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VALID_CONTINUATIONS: &[&str] = &[
    ".min(",
    ".max(",
    ".email(",
    ".url(",
    ".uuid(",
    ".regex(",
    ".length(",
    ".startsWith(",
    ".endsWith(",
    ".optional(",
    ".nullable(",
    ".nullish(",
    ".trim(",
    ".toLowerCase(",
    ".toUpperCase(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("z.string()") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("z.string()") {
                continue;
            }
            if line.trim().starts_with("//") {
                continue;
            }
            if let Some(pos) = line.find("z.string()") {
                let after = &line[pos + "z.string()".len()..];
                if VALID_CONTINUATIONS.iter().any(|c| after.contains(c)) {
                    continue;
                }
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: pos + 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Bare `z.string()` accepts empty strings — add `.min(1)` or a format constraint."
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
        Check.check(&CheckCtx::for_test(Path::new("schema.ts"), src))
    }

    #[test]
    fn flags_bare_string() {
        assert_eq!(run("const s = z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_min() {
        assert!(run("z.string().min(1)").is_empty());
    }

    #[test]
    fn allows_email() {
        assert!(run("z.string().email()").is_empty());
    }

    #[test]
    fn allows_optional() {
        assert!(run("z.string().optional()").is_empty());
    }
}
