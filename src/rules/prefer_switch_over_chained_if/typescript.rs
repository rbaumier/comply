//! prefer-switch-over-chained-if backend — flag 4+ `if / else if` branches
//! testing the same discriminant.
//!
//! Why: a chain like
//!     if (k === 'a') foo();
//!     else if (k === 'b') bar();
//!     else if (k === 'c') baz();
//!     else if (k === 'd') qux();
//! is a switch statement in disguise. The `switch` form makes the
//! discriminant obvious to the reader at a glance, and the compiler can
//! warn on missing cases when the discriminant has a union type.
//!
//! Detection: walk `if_statement` nodes that aren't themselves the `else`
//! branch of another `if_statement` (only count chain roots). Count the
//! consecutive `else if` branches. Flag chains with 4+ total arms.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const DEFAULT_MIN_ARMS: usize = 4;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let min_arms = ctx.config.threshold(
            "prefer-switch-over-chained-if",
            "min_arms",
            DEFAULT_MIN_ARMS,
        );
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "if_statement" {
                return;
            }
            // Only count chain roots — skip nested if-statements that are
            // themselves the else-branch of another if.
            if is_else_branch(node) {
                return;
            }
            let arms = count_chained_arms(node);
            if arms < min_arms {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-switch-over-chained-if".into(),
                message: format!(
                    "{arms}-branch if/else-if chain — convert to a \
                     `switch` statement. Switch makes the discriminant \
                     obvious and the TypeScript compiler can warn on \
                     missing cases for union-typed values."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// True if this if_statement is directly the `else` branch of another if.
fn is_else_branch(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "else_clause" {
        return false;
    }
    parent.parent().is_some_and(|p| p.kind() == "if_statement")
}

/// Count the arms of an if/else-if chain starting at the given root.
fn count_chained_arms(node: tree_sitter::Node) -> usize {
    let mut arms = 1;
    let mut current = node;
    while let Some(alt) = current.child_by_field_name("alternative") {
        // `else_clause` wraps the alternative; if its body is another
        // if_statement, that's an `else if` arm.
        let Some(inner) = alt.named_child(0) else {
            break;
        };
        if inner.kind() != "if_statement" {
            break;
        }
        arms += 1;
        current = inner;
    }
    arms
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
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_four_arm_chain() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
    else if (k === 'd') return 4;
}
";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_three_arm_chain() {
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
}
";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_single_if() {
        assert!(run_on("function f() { if (x) return 1; }").is_empty());
    }

    #[test]
    fn does_not_double_count_nested_chain() {
        // The inner `else if` shouldn't be counted as its own chain root.
        let source = "
function f(k: string) {
    if (k === 'a') return 1;
    else if (k === 'b') return 2;
    else if (k === 'c') return 3;
    else if (k === 'd') return 4;
    else if (k === 'e') return 5;
}
";
        assert_eq!(run_on(source).len(), 1);
    }
}
