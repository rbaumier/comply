//! function-inside-loop Rust backend.
//!
//! Flag closure definitions inside loop bodies.
//! In Rust, `fn` items inside loops are compile-time constructs and don't
//! allocate per iteration, but closures do. We flag closure_expression
//! nodes that are direct children of loop bodies.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const LOOP_KINDS: &[&str] = &[
    "for_expression",
    "while_expression",
    "loop_expression",
];

#[derive(Debug)]
pub struct Check;

fn is_inside_loop(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if LOOP_KINDS.contains(&parent.kind()) {
            return true;
        }
        // Stop at function boundaries.
        if parent.kind() == "function_item" || parent.kind() == "closure_expression" {
            return false;
        }
        current = parent.parent();
    }
    false
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "closure_expression" {
                return;
            }
            if !is_inside_loop(node) {
                return;
            }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "function-inside-loop".into(),
                message: "Closure defined inside a loop \u{2014} move it outside to avoid allocating on every iteration.".into(),
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
    fn flags_closure_in_for_loop() {
        let src = r#"
fn f() {
    for i in 0..10 {
        let inner = |x| x + i;
        inner(1);
    }
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_closure_outside_loop() {
        let src = r#"
fn f() {
    let closure = |x| x + 1;
    for i in 0..10 {
        closure(i);
    }
}
"#;
        assert!(run_on(src).is_empty());
    }
}
