//! no-invariant-returns AST backend — flag functions that always return
//! the same literal value.
//!
//! Walks function-kind AST nodes (`function_declaration`, `function`,
//! `function_expression`, `arrow_function`, `method_definition`,
//! `generator_function*`), collects only the `return_statement` nodes that
//! belong directly to that function (descending through control-flow
//! constructs but skipping nested function/arrow bodies), and flags the
//! function when every return carries the same literal value.

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

/// Recursively collect `return_statement` nodes directly belonging to the
/// enclosing function — skipping any nested function/arrow body so an
/// inner callback's returns are not attributed to the outer function.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        if is_function_kind(child.kind()) {
            continue;
        }
        if child.kind() == "return_statement" {
            out.push(child);
            continue;
        }
        collect_returns(child, out);
    }
}

/// Extract a normalized literal text from a return statement's value node.
/// Returns `None` when the return has no value or the value is not a literal
/// we can compare structurally (numbers, strings, `true`/`false`/`null`/
/// `undefined`).
fn return_literal_text<'a>(ret: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    // `return_statement` has the value as its first named child.
    let value = ret.named_child(0)?;
    let kind = value.kind();
    let text = value.utf8_text(source).ok()?.trim();
    match kind {
        "number" | "string" | "true" | "false" | "null" => Some(text),
        // `undefined` shows up as an `identifier` in the TS grammar.
        "identifier" if text == "undefined" => Some(text),
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_function_kind(node.kind()) {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    if returns.len() < 2 {
        return;
    }

    let mut literals: Vec<&str> = Vec::with_capacity(returns.len());
    for ret in &returns {
        let Some(lit) = return_literal_text(*ret, source) else {
            return; // Non-literal return — bail out, can't prove invariance.
        };
        literals.push(lit);
    }

    let first = literals[0];
    if !literals.iter().all(|l| *l == first) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invariant-returns".into(),
        message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_invariant_true() {
        let src = r#"
function isEnabled(x) {
    if (x > 0) {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_invariant_number() {
        let src = r#"
function getDefault(mode) {
    if (mode === "a") {
        return 0;
    }
    return 0;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
function isPositive(n) {
    if (n > 0) {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
function getValue() {
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
