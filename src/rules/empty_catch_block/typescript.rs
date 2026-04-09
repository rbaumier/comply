//! empty-catch-block backend for TypeScript / JavaScript / TSX.
//!
//! Walks every `catch_clause` and emits a diagnostic when its body is an
//! empty `statement_block` — i.e. `catch (e) {}` or `catch {}`. A body
//! containing even one statement (including a single comment-bearing no-op)
//! is allowed; the rule's job is to catch the silent-swallow pattern, not
//! to police what counts as "enough" recovery.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "catch_clause" {
                return;
            }
            // The body is the `statement_block` child. If it has zero
            // named children (no statements), the catch swallows silently.
            let Some(body) = node.child_by_field_name("body") else {
                return;
            };
            if body.kind() != "statement_block" {
                return;
            }
            if body.named_child_count() > 0 {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "empty-catch-block".into(),
                message: "Empty catch block swallows errors silently. Either rethrow \
                          with context, log and recover, or convert to a Result error. \
                          If the error truly doesn't matter, add a comment explaining why."
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
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source), &tree)
    }

    #[test]
    fn flags_empty_catch_with_param() {
        let diags = run_on("try { f(); } catch (e) {}");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "empty-catch-block");
    }

    #[test]
    fn flags_empty_catch_without_param() {
        let diags = run_on("try { f(); } catch {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_catch_with_statement() {
        let diags = run_on("try { f(); } catch (e) { console.error(e); }");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_catch_with_rethrow() {
        let diags = run_on("try { f(); } catch (e) { throw new Error('wrap', { cause: e }); }");
        assert!(diags.is_empty());
    }
}
