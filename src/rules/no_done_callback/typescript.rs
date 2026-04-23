//! no-done-callback backend — flag `test("...", (done) => {...})` and
//! variants like `it.only("...", function(done) {...})`.
//!
//! The legacy Mocha/Jest callback-based async pattern passes a `done`
//! function into the test body. Modern runners support async/await,
//! which is harder to forget to call and fails loudly on unhandled
//! rejections. Detect any test callback that takes a parameter.

use crate::diagnostic::{Diagnostic, Severity};

/// Recognised test entry points whose second argument is a test body.
const TEST_BASES: &[&str] = &["test", "it"];

/// Modifier methods (`test.only`, `it.skip`) that still take a callback
/// as the second argument.
const TEST_MODIFIERS: &[&str] = &["only", "skip"];

/// Return true if `node` is a `test`/`it` identifier or a
/// `test.only` / `it.skip` member expression.
fn is_test_callee(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "identifier" => {
            let name = node.utf8_text(source).unwrap_or("");
            TEST_BASES.contains(&name)
        }
        "member_expression" => {
            let Some(object) = node.child_by_field_name("object") else {
                return false;
            };
            let Some(property) = node.child_by_field_name("property") else {
                return false;
            };
            if object.kind() != "identifier" {
                return false;
            }
            let base = object.utf8_text(source).unwrap_or("");
            let method = property.utf8_text(source).unwrap_or("");
            TEST_BASES.contains(&base) && TEST_MODIFIERS.contains(&method)
        }
        _ => false,
    }
}

/// Return true if the given function/arrow node has at least one
/// parameter declared. Both `arrow_function` and `function`/
/// `function_expression` expose their parameters via the
/// `parameters` field.
fn has_any_parameter(func: tree_sitter::Node) -> bool {
    let Some(params) = func.child_by_field_name("parameters") else {
        return false;
    };
    params.named_child_count() > 0
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else {
        return;
    };
    if !is_test_callee(callee, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let Some(callback) = args.named_child(1) else {
        return;
    };
    if !matches!(
        callback.kind(),
        "arrow_function" | "function" | "function_expression"
    ) {
        return;
    }

    if !has_any_parameter(callback) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-done-callback".into(),
        message: "Test callback takes a `done`-style parameter — use async/await instead."
            .into(),
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
    fn flags_test_with_done_arrow() {
        let src = "test('x', (done) => { done(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_it_with_done_function_expr() {
        let src = "it('x', function(done) { done(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_test_only_with_done() {
        let src = "test.only('x', (done) => { done(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_it_skip_with_done() {
        let src = "it.skip('x', (done) => { done(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_async_test() {
        let src = "test('x', async () => { await doThing(); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_test_with_no_params() {
        let src = "test('x', () => { expect(1).toBe(1); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_test_function_with_param() {
        let src = "myHelper('x', (arg) => { return arg; });";
        assert!(run_on(src).is_empty());
    }
}
