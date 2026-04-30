//! CSS backend — flag `@plugin "tailwindcss-animate"` (Tailwind v4) and
//! `@import "tailwindcss-animate"` directives.
//!
//! tree-sitter-css doesn't recognise the Tailwind v4 `@plugin` directive,
//! so it lands inside an `ERROR` node that the AST walker skips. A plain
//! text scan keeps the implementation honest without a brittle parser
//! workaround.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const FORBIDDEN: &str = "tailwindcss-animate";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            // Only fire on at-rules that reference the package — comments and
            // unrelated mentions stay quiet.
            let looks_like_directive = trimmed.starts_with("@plugin")
                || trimmed.starts_with("@import")
                || trimmed.starts_with("@use");
            if !looks_like_directive {
                continue;
            }
            if !line.contains(FORBIDDEN) {
                continue;
            }
            let column = line.find(FORBIDDEN).unwrap_or(0) + 1;
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: idx + 1,
                column,
                rule_id: super::META.id.into(),
                message: "`tailwindcss-animate` is unmaintained for Tailwind v4 — use `tw-animate-css` instead.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let path = Path::new("t.css");
        let ctx = CheckCtx::for_test(path, source);
        Check.check(&ctx)
    }

    #[test]
    fn flags_plugin_directive() {
        assert_eq!(run(r#"@plugin "tailwindcss-animate";"#).len(), 1);
    }

    #[test]
    fn flags_import() {
        assert_eq!(run(r#"@import "tailwindcss-animate";"#).len(), 1);
    }

    #[test]
    fn allows_tw_animate_css() {
        assert!(run(r#"@plugin "tw-animate-css";"#).is_empty());
    }
}
