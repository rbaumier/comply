//! zod-refine-requires-path backend — only runs when the file mentions
//! both `z.object(` and `.refine(`. Flags any `.refine(` invocation on a
//! line carrying a `message` but no `path:` key, which is the footgun
//! that puts form errors on the root object instead of a field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("z.object(") || !ctx.source.contains(".refine(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains(".refine(") || t.starts_with("//") {
                continue;
            }
            if t.contains("path:") || t.contains("path :") {
                continue;
            }
            if t.contains(".refine(") && t.contains("message") && !t.contains("path") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find(".refine(").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Add `path: ['fieldName']` to `.refine()` options so form errors attach to the correct field."
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
    fn flags_refine_no_path() {
        assert_eq!(
            run("z.object({ a: z.string(), b: z.string() }).refine(d => d.a !== d.b, { message: 'Must differ' })").len(),
            1
        );
    }

    #[test]
    fn allows_refine_with_path() {
        assert!(run(
            "z.object({ a: z.string() }).refine(d => d.a.length > 0, { message: 'Required', path: ['a'] })"
        )
        .is_empty());
    }
}
