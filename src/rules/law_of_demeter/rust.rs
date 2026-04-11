//! law-of-demeter Rust backend.
//!
//! Flags deep field_expression chains (not method chains — those are
//! idiomatic in Rust builders/iterators). Only flags `a.b.c.d` where
//! segments are field accesses (not method calls).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const DEFAULT_MAX_DEPTH: usize = 3;

#[derive(Debug)]
pub struct Check;

fn chain_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node;
    while current.kind() == "field_expression" {
        depth += 1;
        let Some(obj) = current.child_by_field_name("value") else { break };
        current = obj;
    }
    depth
}

fn is_self_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while current.kind() == "field_expression" {
        let Some(obj) = current.child_by_field_name("value") else { break };
        current = obj;
    }
    let Ok(text) = current.utf8_text(source) else { return false };
    text == "self" || text == "Self"
}

fn is_all_uppercase(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else { return false };
    text.chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| c.is_uppercase())
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let max_depth = ctx
            .config
            .threshold("law-of-demeter", "max_depth", DEFAULT_MAX_DEPTH);
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "field_expression" {
                return;
            }
            // Skip inner field expressions — only analyze outermost.
            if node
                .parent()
                .is_some_and(|p| p.kind() == "field_expression")
            {
                return;
            }
            let depth = chain_depth(node);
            if depth < max_depth {
                return;
            }
            if is_self_chain(node, source) || is_all_uppercase(node, source) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "law-of-demeter".into(),
                message: format!(
                    "Chained field access {depth} levels deep — Law of Demeter violation."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_deep_field_chain() {
        let src = "fn f(o: &Obj) { let _ = o.customer.address.city; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_self_chain() {
        let src = "fn f(&self) { let _ = self.config.db.url; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_short_chain() {
        let src = "fn f(o: &Obj) { let _ = o.name; }";
        assert!(run_on(src).is_empty());
    }
}
