//! no-throw backend for TypeScript / JavaScript / TSX.
//!
//! Walks the AST looking for every `throw_statement` node and emits one
//! diagnostic per occurrence. Covers all three TS-family grammars since
//! tree-sitter-typescript parses plain JS too; TSX uses its own grammar
//! variant but exposes the same `throw_statement` kind.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "throw_statement" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-throw".into(),
                message: "Use Result<T, E> instead of throw — surface errors as values, \
                          not exceptions. Callers can't see thrown errors in the type signature."
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
    fn flags_throw_statement() {
        let diags = run_on("function f() { throw new Error('boom'); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-throw");
    }

    #[test]
    fn allows_code_without_throw() {
        assert!(run_on("function f() { return 42; }").is_empty());
    }

    #[test]
    fn flags_multiple_throws() {
        let diags = run_on("function f() { throw 1; } function g() { throw 2; }");
        assert_eq!(diags.len(), 2);
    }
}
