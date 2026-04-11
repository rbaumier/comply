//! nested-control-flow Rust backend.
//!
//! Flag control-flow nesting deeper than 3.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const MAX_DEPTH: usize = 3;

const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    "match_expression",
];

#[derive(Debug)]
pub struct Check;

fn control_flow_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if CONTROL_FLOW_KINDS.contains(&ancestor.kind()) {
            depth += 1;
        }
        current = ancestor.parent();
    }
    depth
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut flagged_lines = std::collections::HashSet::new();

        walk_tree(tree, |node| {
            if !CONTROL_FLOW_KINDS.contains(&node.kind()) {
                return;
            }
            let depth = control_flow_depth(node) + 1;
            if depth > MAX_DEPTH {
                let line = node.start_position().row + 1;
                if flagged_lines.insert(line) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line,
                        column: node.start_position().column + 1,
                        rule_id: "nested-control-flow".into(),
                        message: format!(
                            "Control-flow nesting depth is {depth} (max: {MAX_DEPTH})."
                        ),
                        severity: Severity::Error,
                    });
                }
            }
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
    fn allows_shallow_nesting() {
        let src = r#"
fn foo() {
    if true {
        if true {
            if true {
                do_something();
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_deep_nesting() {
        let src = r#"
fn foo() {
    if true {
        if true {
            if true {
                if true {
                    do_something();
                }
            }
        }
    }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("4"));
    }

    #[test]
    fn counts_mixed_control_flow() {
        let src = r#"
fn bar() {
    for x in items.iter() {
        while condition {
            if check {
                match val {
                    _ => {}
                }
            }
        }
    }
}
"#;
        let diags = run_on(src);
        assert!(!diags.is_empty());
    }
}
