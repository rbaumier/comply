//! tailwind-no-arbitrary-z-index backend — flag arbitrary numeric z-index
//! values like `z-[100]` that bypass the Tailwind scale and grow unbounded.
//! Named arbitrary values (`z-[var(--modal)]`) are left alone since they
//! already route through a design token.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") {
                continue;
            }
            if let Some(pos) = line.find("z-[") {
                let after = &line[pos + 3..];
                if after.starts_with(|c: char| c.is_ascii_digit()) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: pos + 1,
                        rule_id: super::META.id.into(),
                        message: "Use a design token (e.g. `z-10`, `z-50`) instead of an arbitrary z-index value.".into(),
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
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_arbitrary_z() {
        assert_eq!(run(r#"<div className="z-[100] relative" />"#).len(), 1);
    }

    #[test]
    fn flags_large_z() {
        assert_eq!(run(r#"<div className="z-[9999]" />"#).len(), 1);
    }

    #[test]
    fn allows_token_z() {
        assert!(run(r#"<div className="z-10 relative" />"#).is_empty());
    }

    #[test]
    fn allows_named_z() {
        assert!(run(r#"<div className="z-modal" />"#).is_empty());
    }
}
