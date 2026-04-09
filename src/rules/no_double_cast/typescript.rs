//! no-double-cast backend — flag `x as unknown as T` style double casts.
//!
//! Why: a double cast is an explicit "I know the type checker disagrees
//! and I'm telling it to shut up". It hides misaligned types behind two
//! `as` hops that bypass every safety check. The real fix is to align
//! the interfaces — refactor the producer or validate at the boundary.
//!
//! Detection: walk `as_expression` nodes whose inner expression is also
//! an `as_expression`. The outer cast is the diagnostic site.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "as_expression" {
                return;
            }
            // The "value" side of an as_expression is the first child.
            let Some(inner) = node.named_child(0) else {
                return;
            };
            if inner.kind() != "as_expression" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-double-cast".into(),
                message: "Double cast `as X as Y` hides misaligned types. \
                          Fix the real problem: align the interface, or \
                          validate at the boundary with a type guard or Zod \
                          schema that actually checks the shape at runtime."
                    .into(),
                severity: Severity::Error,
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
    fn flags_as_unknown_as_t() {
        assert_eq!(run_on("const x = value as unknown as MyType;").len(), 1);
    }

    #[test]
    fn flags_as_any_as_t() {
        assert_eq!(run_on("const x = value as any as User;").len(), 1);
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("const x = value as MyType;").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(run_on("const x = [1, 2, 3] as const;").is_empty());
    }
}
