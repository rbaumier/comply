//! no-test-return-statement backend — flag `return` inside test/it callbacks.
//!
//! A test callback that returns a value is a smell: either the test body is
//! doing work whose result the runner ignores, or the author meant to `await`
//! a promise and wrote `return promise` instead of adding `expect` assertions.
//! Either way, the fix is to drop the `return` and assert the outcome.
//!
//! Detection: walk up from a `return_statement` and look at the first
//! enclosing function (`arrow_function` / `function_expression` / `function`).
//! If that function is the direct callback argument of a `test(...)` or
//! `it(...)` call expression, the return is inside the test body and we flag
//! it. Returns inside nested helpers declared within the test are ignored —
//! the first enclosing function there is the helper, not the test callback.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_FNS: &[&str] = &["test", "it"];

/// True when `node` is a `return_statement` whose nearest enclosing function
/// is the callback passed directly to a `test(...)` or `it(...)` call.
fn is_return_in_test_callback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "arrow_function"
            | "function_expression"
            | "function"
            | "function_declaration"
            | "method_definition" => {
                // First enclosing function found. Check that this function is
                // an argument to a test(...)/it(...) call.
                let Some(call) = p.parent().and_then(|args| {
                    if args.kind() == "arguments" {
                        args.parent()
                    } else {
                        None
                    }
                }) else {
                    return false;
                };
                if call.kind() != "call_expression" {
                    return false;
                }
                let Some(callee) = call.child_by_field_name("function") else {
                    return false;
                };
                let name = match callee.kind() {
                    "identifier" => callee.utf8_text(source).unwrap_or(""),
                    _ => return false,
                };
                return TEST_FNS.contains(&name);
            }
            _ => {}
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { on ["return_statement"] => |node, source, ctx, diagnostics|
    if !is_return_in_test_callback(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-test-return-statement".into(),
        message: "Remove `return` from test body — use `expect` assertions instead.".into(),
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
    fn flags_return_in_test_arrow() {
        let d = run_on("test('x', () => { return 42; });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-test-return-statement");
    }

    #[test]
    fn flags_return_in_it_function_expression() {
        let d = run_on("it('x', function () { return someVar; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_return_in_nested_function() {
        let d = run_on(
            "test('x', () => { const helper = () => { return 1; }; expect(helper()).toBe(1); });",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_test_without_return() {
        let d = run_on("test('x', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_return_outside_test() {
        let d = run_on("function foo() { return 1; }");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_return_in_nested_function_declaration() {
        let d = run_on(
            "test('x', () => { function Page() { return <div/>; } expect(Page).toBeDefined(); });",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_return_in_object_method() {
        let d = run_on(
            "test('x', () => { const cfg = { init() { return cleanup; } }; expect(cfg).toBeDefined(); });",
        );
        assert!(d.is_empty());
    }
}
