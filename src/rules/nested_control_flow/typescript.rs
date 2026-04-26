//! nested-control-flow TypeScript / JavaScript / TSX backend.
//!
//! Counts ancestors of each control-flow node up to the nearest function
//! boundary, collapses `else if` cascades, and flags depth > MAX_DEPTH.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const CONTROL_FLOW_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
    "try_statement",
];

/// Scopes that reset the depth counter. Matches eslint `max-depth`:
/// reset on every callable (function declarations, function expressions,
/// arrow functions, method definitions, generators).
const FN_RESET_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

#[derive(Debug)]
pub struct Check;

/// Count control-flow ancestors of `node` up to the nearest function
/// boundary. An `if_statement` reached via its own `else_clause` child is
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
                parent.kind() == "if_statement" && current.kind() == "else_clause";
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

        // Skip the inner `if_statement` of an `else if` cascade — it is
        // the same cognitive level as the outer `if`, counted once.
        if node.kind() == "if_statement"
            && let Some(parent) = node.parent()
                && parent.kind() == "else_clause" {
                    return;
                }
        let depth = control_flow_depth(node) + 1;
        if depth > max_depth {
            let line = node.start_position().row + 1;
            if flagged_lines.insert(line) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_shallow_nesting() {
        let src = r#"
function foo() {
    if (a) {
        if (b) {
            if (c) {
                doSomething();
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
function foo() {
    if (a) {
        if (b) {
            if (c) {
                if (d) {
                    doSomething();
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
function bar() {
    for (const x of items) {
        while (condition) {
            try {
                if (check) {
                    boom();
                }
            } catch (e) {}
        }
    }
}
"#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_non_control_flow_braces() {
        let src = r#"
function baz() {
    if (a) {
        const obj = { key: { nested: { deep: true } } };
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// A 5-branch `else if` cascade is one cognitive level, not five.
    #[test]
    fn allows_five_branch_else_if_cascade() {
        let src = r#"
function classify(ext) {
    if (ext === "ts") {
        return 1;
    } else if (ext === "tsx") {
        return 2;
    } else if (ext === "js") {
        return 3;
    } else if (ext === "rs") {
        return 4;
    } else if (ext === "vue") {
        return 5;
    } else {
        return null;
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    /// Arrow function body has its own depth counter.
    #[test]
    fn arrow_body_resets_depth() {
        let src = r#"
function outer() {
    for (const _ of a) {
        for (const _ of b) {
            for (const _ of c) {
                const cb = (x) => {
                    if (x > 0) {
                        if (x > 1) {
                            if (x > 2) {
                                doSomething();
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
        assert!(run_on(src).is_empty());
    }

    /// Nested function declaration also resets the counter.
    #[test]
    fn nested_fn_resets_depth() {
        let src = r#"
function outer() {
    for (const _ of a) {
        for (const _ of b) {
            for (const _ of c) {
                function inner() {
                    if (true) {
                        if (true) {
                            if (true) {
                                doSomething();
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

    #[test]
    fn flags_deep_nesting_inside_arrow() {
        let src = r#"
function outer() {
    const cb = (x) => {
        if (x > 0) {
            if (x > 1) {
                if (x > 2) {
                    if (x > 3) {
                        doSomething();
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
