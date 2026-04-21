//! zod-no-optional-nullable-chain backend — detect the redundant pairing
//! of `.optional()` and `.nullable()` in either order. Both orderings
//! collapse to the built-in `.nullish()`, so surfacing them nudges
//! authors toward the clearer form.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            let has_chain = t.contains(".optional().nullable()")
                || t.contains(".nullable().optional()");
            if has_chain {
                let col = line
                    .find(".optional().nullable()")
                    .or_else(|| line.find(".nullable().optional()"))
                    .unwrap_or(0);
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Replace `.optional().nullable()` with `.nullish()` for clearer intent."
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
    fn flags_optional_nullable() {
        assert_eq!(run("z.string().optional().nullable()").len(), 1);
    }

    #[test]
    fn flags_nullable_optional() {
        assert_eq!(run("z.string().nullable().optional()").len(), 1);
    }

    #[test]
    fn allows_nullish() {
        assert!(run("z.string().nullish()").is_empty());
    }
}
