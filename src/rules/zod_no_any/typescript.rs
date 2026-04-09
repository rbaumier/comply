//! zod-no-any backend — flag `z.any()`.
//!
//! Why: `z.any()` accepts anything — it's a type escape hatch that
//! disables validation entirely. Use `z.unknown()` instead: the runtime
//! result is the same, but the TypeScript type is `unknown`, forcing
//! downstream code to narrow before using the value.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            let Ok(fn_text) = function.utf8_text(source_bytes) else {
                return;
            };
            if fn_text != "z.any" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "zod-no-any".into(),
                message: "`z.any()` disables validation — use `z.unknown()` \
                          so the TypeScript type forces downstream code to \
                          narrow before using the value."
                    .into(),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.ts"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_z_any() {
        assert_eq!(run_on("const s = z.any();").len(), 1);
    }

    #[test]
    fn allows_z_unknown() {
        assert!(run_on("const s = z.unknown();").is_empty());
    }
}
