//! no-inconsistent-returns AST backend — flag functions that mix
//! `return expr;` with bare `return;`.
//!
//! Walks tree-sitter AST nodes for every function-kind node
//! (declarations, expressions, arrows, methods, generators) and collects
//! only their direct `return_statement` children. Nested function/arrow
//! bodies are skipped so an inner callback's return is not attributed
//! to its enclosing function.
//!
//! A `return_statement` with a named child returns a value; otherwise bare.

use crate::diagnostic::{Diagnostic, Severity};

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

fn is_function_kind(kind: &str) -> bool {
    FUNCTION_KINDS.contains(&kind)
}

/// Walk `node`'s subtree collecting `return_statement` nodes that belong
/// directly to `node` — descend into control-flow constructs (if, blocks,
/// loops, try, switch, ...) but stop at any nested function-kind node.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        let kind = child.kind();
        if is_function_kind(kind) {
            // Inner function/arrow: its returns belong to it, not us.
            continue;
        }
        if kind == "return_statement" {
            out.push(child);
            // Don't descend — any expression inside is the return value.
            continue;
        }
        collect_returns(child, out);
    }
}

/// True if a `return_statement` carries a value (has a named child).
fn return_has_value(ret: tree_sitter::Node) -> bool {
    ret.named_child_count() > 0
}

crate::ast_check! { on ["function_declaration", "function_expression", "function", "arrow_function", "method_definition", "generator_function_declaration", "generator_function"] => |node, _source, ctx, diagnostics|
    // Find the body. For declarations/methods/expressions/generators the
    // body field is "body" pointing at a statement_block. For arrow
    // functions the body may be a statement_block or a bare expression
    // (no return_statement possible in the latter).
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    let mut has_value = false;
    let mut has_bare = false;
    for ret in &returns {
        if return_has_value(*ret) {
            has_value = true;
        } else {
            has_bare = true;
        }
    }

    if has_value && has_bare {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-inconsistent-returns".into(),
            message: "Function has inconsistent returns — some paths return a value, others return nothing.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_mixed_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_consistent_value_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return 42;
    }
    return 0;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_consistent_bare_returns() {
        let code = r#"
function foo(x) {
    if (x) {
        return;
    }
    return;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_async_function() {
        let code = r#"
async function fetchData(url) {
    if (!url) {
        return;
    }
    return fetch(url);
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_arrow_returns_to_outer_fn() {
        // Outer fn has only `return 1;`. Inner arrow has `return;`.
        let code = r#"
function outer() {
    const cb = (x) => {
        if (x === 0) {
            return;
        }
        console.log(x);
    };
    return 1;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_arrow_with_inconsistent_returns() {
        let code = r#"
const f = (x) => {
    if (x === 0) {
        return;
    }
    return x + 1;
};
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_method_shorthand_returns_to_outer() {
        let code = r#"
function outer() {
    const obj = {
        foo() {
            if (true) return;
            console.log("ok");
        },
    };
    return 1;
}
"#;
        assert!(run_on(code).is_empty());
    }
}
