//! prefer-array-index-of AST backend — flag `.findIndex(x => x === val)`.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[&str] = &["findIndex", "findLastIndex"];

crate::ast_check! { on ["call_expression"] prefilter = ["findIndex", "findLastIndex"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !METHODS.contains(&method) {
        return;
    }

    // Get the callback argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first = args.children(&mut cursor)
        .find(|c| !matches!(c.kind(), "(" | ")" | ","));
    let Some(callback) = first else { return };

    if callback.kind() != "arrow_function" {
        return;
    }

    // Get the parameter name.
    let Some(params) = callback.child_by_field_name("parameters") else {
        // Single bare parameter (no parens): first child is the parameter.
        let Some(param) = callback.child_by_field_name("parameter") else { return };
        let param_name = param.utf8_text(source).unwrap_or("");
        if param_name.is_empty() { return; }
        // Get the body.
        let Some(body) = callback.child_by_field_name("body") else { return };
        if !is_simple_equality(body, param_name, source) { return; }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-array-index-of".into(),
            message: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    };

    // Parenthesized params — must be exactly one.
    let mut pc = params.walk();
    let param_nodes: Vec<_> = params.children(&mut pc)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();
    if param_nodes.len() != 1 {
        return;
    }
    let param_name = param_nodes[0].utf8_text(source).unwrap_or("");
    if param_name.is_empty() {
        return;
    }

    // Get the body.
    let Some(body) = callback.child_by_field_name("body") else { return };
    if !is_simple_equality(body, param_name, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-array-index-of".into(),
        message: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// Check if a node is a simple `param === something` or `something === param`
/// binary expression (strict equality).
fn is_simple_equality(node: tree_sitter::Node, param: &str, source: &[u8]) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }

    // Check for `===` operator.
    let mut cursor = node.walk();
    let has_strict_eq = node
        .children(&mut cursor)
        .any(|c| c.kind() == "===" || c.utf8_text(source).unwrap_or("") == "===");
    if !has_strict_eq {
        return false;
    }

    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return false;
    };

    let left_text = left.utf8_text(source).unwrap_or("");
    let right_text = right.utf8_text(source).unwrap_or("");

    // One side must be exactly the parameter (a simple identifier).
    if left_text == param && left.kind() == "identifier" {
        // Right side must also be a simple identifier (not a member expression).
        return right.kind() == "identifier";
    }
    if right_text == param && right.kind() == "identifier" {
        return left.kind() == "identifier";
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_findindex_arrow_equality() {
        assert_eq!(run_on("const i = arr.findIndex(x => x === val);").len(), 1);
    }

    #[test]
    fn flags_findindex_parens_arrow() {
        assert_eq!(
            run_on("const i = arr.findIndex((x) => x === val);").len(),
            1
        );
    }

    #[test]
    fn flags_findindex_reversed_comparison() {
        assert_eq!(run_on("const i = arr.findIndex(x => val === x);").len(), 1);
    }

    #[test]
    fn flags_findlastindex() {
        assert_eq!(
            run_on("const i = arr.findLastIndex(x => x === val);").len(),
            1
        );
    }

    #[test]
    fn allows_indexof() {
        assert!(run_on("const i = arr.indexOf(val);").is_empty());
    }

    #[test]
    fn allows_complex_callback() {
        assert!(run_on("const i = arr.findIndex(x => x.id === val);").is_empty());
    }
}
