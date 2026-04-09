//! law-of-demeter backend for Rust.
//!
//! Flags method call chains 3+ levels deep on an external value
//! (`order.customer().address().city()`). `self.*` and all-caps constant
//! chains are exempt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const DEFAULT_MAX_DEPTH: usize = 3;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let max_depth = ctx
            .config
            .threshold("law-of-demeter", "max_depth", DEFAULT_MAX_DEPTH);
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "call_expression" {
                return;
            }
            // Only analyze outermost chain root — skip intermediate calls
            // that are themselves part of a parent chain.
            if parent_is_call_chain(node) {
                return;
            }
            let depth = chain_depth(node);
            if depth < max_depth {
                return;
            }
            if is_self_chain(node, source_bytes) || is_constant_chain(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "law-of-demeter".into(),
                message: format!(
                    "Method call chain {depth} levels deep — Law of Demeter \
                     violation. Add a direct accessor on the immediate \
                     dependency instead of reaching through."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// True if this call_expression is the receiver of another call_expression.
fn parent_is_call_chain(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "field_expression" {
        // field_expression is the receiver of the parent call — keep climbing.
        return parent
            .parent()
            .is_some_and(|gp| gp.kind() == "call_expression");
    }
    parent.kind() == "call_expression"
}

/// Count `.method()` segments in the chain rooted at this call.
fn chain_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node;
    loop {
        if current.kind() == "call_expression" {
            depth += 1;
            // Drop into the callee's receiver (field_expression → object).
            let Some(function) = current.child_by_field_name("function") else {
                break;
            };
            if function.kind() == "field_expression"
                && let Some(value) = function.child_by_field_name("value")
            {
                current = value;
                continue;
            }
            break;
        }
        break;
    }
    depth
}

/// True if the chain's innermost receiver is `self`.
fn is_self_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while current.kind() == "call_expression" {
        let Some(function) = current.child_by_field_name("function") else {
            return false;
        };
        if function.kind() == "field_expression"
            && let Some(value) = function.child_by_field_name("value")
        {
            current = value;
            continue;
        }
        return false;
    }
    current.utf8_text(source).is_ok_and(|t| t == "self")
}

/// True if every receiver segment is an UPPERCASE constant.
fn is_constant_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node;
    while current.kind() == "call_expression" {
        let Some(function) = current.child_by_field_name("function") else {
            return false;
        };
        if function.kind() == "field_expression"
            && let Some(value) = function.child_by_field_name("value")
        {
            current = value;
            continue;
        }
        return false;
    }
    current
        .utf8_text(source)
        .is_ok_and(is_screaming_snake_case)
}

fn is_screaming_snake_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_three_level_chain() {
        assert_eq!(
            run_on("fn f() { let x = order.customer().address().city(); }").len(),
            1
        );
    }

    #[test]
    fn allows_two_level_chain() {
        assert!(run_on("fn f() { let x = order.customer().name(); }").is_empty());
    }

    #[test]
    fn allows_self_chain() {
        assert!(
            run_on("impl S { fn m(&self) { self.a().b().c().d(); } }").is_empty()
        );
    }
}
