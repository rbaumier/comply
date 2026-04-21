//! zod-prefer-discriminated-union backend — scan sources for
//! `z.union([...])` blocks and flag those whose branches include a shared
//! literal tag (`type: z.literal(...)`, `kind: …`, `__type: …`). A
//! multi-line state machine tracks entry/exit of the union so we can
//! inspect the body without a full AST.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut in_union = false;
        let mut union_start = 0;
        let mut has_literal = false;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            if t.contains("z.union([") && !t.contains("z.discriminatedUnion") {
                in_union = true;
                union_start = i;
                has_literal = false;
            }
            if in_union
                && (t.contains("type: z.literal(")
                    || t.contains("kind: z.literal(")
                    || t.contains("__type: z.literal("))
            {
                has_literal = true;
            }
            if in_union && t.contains("])") {
                if has_literal {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: union_start + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Replace `z.union([z.object({type: z.literal(...)}), ...])` with `z.discriminatedUnion('type', [...])` for faster parsing.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                in_union = false;
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
    fn flags_union_with_literals() {
        let src = "z.union([\n  z.object({ type: z.literal('a') }),\n  z.object({ type: z.literal('b') }),\n])";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_discriminated_union() {
        assert!(
            run("z.discriminatedUnion('type', [z.object({ type: z.literal('a') })])").is_empty()
        );
    }
}
