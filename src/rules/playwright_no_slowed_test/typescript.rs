//! playwright-no-slowed-test AST backend.
//!
//! Flags every zero-argument `test.slow()` call. The unconditional form is
//! a declaration that the test is permanently slow — usually a signal that
//! the test should be optimized rather than blessed. The conditional form
//! `test.slow(condition, reason)` remains allowed because it carries a
//! rationale and runs situationally.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }
    if !is_test_slow_unconditional(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`test.slow()` without arguments marks the test as always slow — optimize it or use the conditional form `test.slow(condition, reason)`.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn is_test_slow_unconditional(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = func.child_by_field_name("object") else {
        return false;
    };
    if obj.kind() != "identifier" || &source[obj.byte_range()] != b"test" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    if prop.kind() != "property_identifier" || &source[prop.byte_range()] != b"slow" {
        return false;
    }
    let arg_count = call
        .child_by_field_name("arguments")
        .map_or(0, |a| a.named_child_count());
    arg_count == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let full = format!("import {{ test, expect }} from \"@playwright/test\";\n{source}");
        crate::rules::test_helpers::run_ts(&full, &Check)
    }

    #[test]
    fn flags_bare_test_slow() {
        let src = "test.slow();";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_test_slow_inside_test() {
        let src = "test('my test', () => { test.slow(); });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_conditional_test_slow() {
        let src = "test('my test', () => { test.slow(process.env.CI, 'CI is slow'); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_normal_test() {
        let src = "test('my test', () => { expect(1).toBe(1); });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_other_slow_methods() {
        let src = "foo.slow();";
        assert!(run_on(src).is_empty());
    }
}
