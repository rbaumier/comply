//! nested-control-flow Rust backend.
//!
//! Counts ancestors of each control-flow node up to the nearest function
//! boundary, collapses `else if` cascades, and flags depth > MAX_DEPTH.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    "match_expression",
];

/// Scopes that reset the depth counter. Matches eslint `max-depth` which
/// resets on every callable (function declarations, function expressions,
/// arrow functions). In Rust that is `function_item` + `closure_expression`.
const FN_RESET_KINDS: &[&str] = &["function_item", "closure_expression"];

#[derive(Debug)]
pub struct Check;

/// Count control-flow ancestors of `node` up to the nearest function
/// boundary. An `if_expression` reached via its own `else_clause` child is
/// not counted — that is an `else if` continuation, visually a flat cascade.
fn control_flow_depth(node: tree_sitter::Node) -> usize {
    let mut depth = 0;
    let mut current = node;
    while let Some(parent) = current.parent() {
        if FN_RESET_KINDS.contains(&parent.kind()) {
            break;
        }
        if CONTROL_FLOW_KINDS.contains(&parent.kind()) {
            let is_else_if_cascade =
                parent.kind() == "if_expression" && current.kind() == "else_clause";
            if !is_else_if_cascade {
                depth += 1;
            }
        }
        current = parent;
    }
    depth
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(CONTROL_FLOW_KINDS)
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(std::collections::HashSet::<usize>::new()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let max_depth = ctx.config.threshold("nested-control-flow", "max");
        let flagged_lines = state
            .unwrap()
            .downcast_mut::<std::collections::HashSet<usize>>()
            .unwrap();

        // Skip the inner `if_expression` of an `else if` cascade — it is
        // the same cognitive level as the outer `if`, counted once.
        if node.kind() == "if_expression"
            && let Some(parent) = node.parent()
                && parent.kind() == "else_clause" {
                    return;
                }
        let depth = control_flow_depth(node) + 1;
        if depth > max_depth {
            let line = node.start_position().row + 1;
            if flagged_lines.insert(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line,
                    column: node.start_position().column + 1,
                    rule_id: "nested-control-flow".into(),
                    message: format!(
                        "Control-flow nesting depth is {depth} (max: {max_depth})."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
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

    /// The FP observed on `src/files.rs` lines 73, 75 (and 171, 173): a
    /// 5-branch `else if` cascade inflated depth to 5. A flat cascade is
    /// the same cognitive level as a single `if`.
    #[test]
    fn allows_five_branch_else_if_cascade() {
        let src = r#"
fn from_path(ext: &str) -> Option<u8> {
    if ext == "ts" {
        Some(1)
    } else if ext == "tsx" {
        Some(2)
    } else if ext == "js" {
        Some(3)
    } else if ext == "rs" {
        Some(4)
    } else if ext == "vue" {
        Some(5)
    } else {
        None
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// `else if` cascade inside a single `for` — depth should be 2, not 6.
    #[test]
    fn allows_else_if_cascade_inside_one_loop() {
        let src = r#"
fn f(items: &[u8]) {
    for &x in items {
        if x == 0 {
            a();
        } else if x == 1 {
            b();
        } else if x == 2 {
            c();
        } else if x == 3 {
            d();
        } else {
            e();
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// A closure body has its own depth counter — outer control-flow does
    /// not leak into the closure.
    #[test]
    fn closure_body_resets_depth() {
        let src = r#"
fn f() {
    for _ in 0..10 {
        for _ in 0..10 {
            for _ in 0..10 {
                let cb = |x: u8| {
                    if x > 0 {
                        if x > 1 {
                            if x > 2 {
                                do_something();
                            }
                        }
                    }
                };
                cb(0);
            }
        }
    }
}
"#;
        // Outer fn: 3 nested for loops → depth 3, not flagged.
        // Closure body: 3 nested ifs → depth 3, not flagged.
        assert!(run_on(src).is_empty());
    }

    /// A nested `fn` declaration also resets the counter.
    #[test]
    fn nested_fn_resets_depth() {
        let src = r#"
fn outer() {
    for _ in 0..10 {
        for _ in 0..10 {
            for _ in 0..10 {
                fn inner() {
                    if true {
                        if true {
                            if true {
                                do_something();
                            }
                        }
                    }
                }
            }
        }
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Nested closure bodies that do exceed depth 3 internally are still
    /// flagged on their own merit.
    #[test]
    fn flags_deep_nesting_inside_closure() {
        let src = r#"
fn f() {
    let cb = |x: u8| {
        if x > 0 {
            if x > 1 {
                if x > 2 {
                    if x > 3 {
                        do_something();
                    }
                }
            }
        }
    };
    cb(0);
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
