//! playwright-prefer-to-have-count — flag `expect(await locator.count()).toBe(n)` patterns.

use crate::diagnostic::{Diagnostic, Severity};

const EQUALITY_MATCHERS: &[&str] = &["toBe", "toEqual", "toStrictEqual"];

/// Returns true if `node` is an `await_expression` wrapping `<obj>.count()`.
fn is_await_count(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "await_expression" {
        return false;
    }
    // await_expression has one child: the awaited expression.
    let Some(inner) = node.named_child(0) else {
        return false;
    };
    if inner.kind() != "call_expression" {
        return false;
    }
    let Some(func) = inner.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = func.child_by_field_name("property") else {
        return false;
    };
    if prop.utf8_text(source).unwrap_or("") != "count" {
        return false;
    }
    // Must have zero arguments.
    let Some(args) = inner.child_by_field_name("arguments") else {
        return false;
    };
    args.named_child_count() == 0
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }
    // We hook on the outer call: expect(await locator.count()).toBe(n)
    // callee must be `<expect-call>.toBe` / `.toEqual` / `.toStrictEqual`
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    let matcher = prop.utf8_text(source).unwrap_or("");
    if !EQUALITY_MATCHERS.contains(&matcher) {
        return;
    }

    // object side must be an `expect(await X.count())` call
    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.kind() != "call_expression" {
        return;
    }
    let Some(obj_fn) = obj.child_by_field_name("function") else { return };
    if obj_fn.kind() != "identifier" {
        return;
    }
    if obj_fn.utf8_text(source).unwrap_or("") != "expect" {
        return;
    }

    // first argument of expect must be `await <locator>.count()`
    let Some(expect_args) = obj.child_by_field_name("arguments") else { return };
    let Some(first_arg) = expect_args.named_child(0) else { return };
    if !is_await_count(first_arg, source) {
        return;
    }

    let pos = obj_fn.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-to-have-count".into(),
        message: "Use `expect(locator).toHaveCount(n)` instead of `expect(await locator.count()).toBe(n)`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    fn pw(s: &str) -> String {
        format!("import {{ test, expect }} from \"@playwright/test\";\n{s}")
    }

    #[test]
    fn flags_await_count_to_be() {
        let d = run_ts(&pw("expect(await locator.count()).toBe(3);"), &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveCount"));
    }

    #[test]
    fn flags_await_count_to_equal() {
        let d = run_ts(
            &pw("expect(await page.locator('.item').count()).toEqual(5);"),
            &Check,
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_await_count_to_strict_equal() {
        let d = run_ts(&pw("expect(await rows.count()).toStrictEqual(0);"), &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_have_count() {
        let d = run_ts(&pw("await expect(locator).toHaveCount(3);"), &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_non_count_await() {
        let d = run_ts(
            &pw("expect(await locator.textContent()).toBe('hello');"),
            &Check,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_count_without_await() {
        // No await → not the target pattern (count() returns Promise, but this code is buggy anyway).
        let d = run_ts("expect(locator.count()).toBe(3);", &Check);
        assert!(d.is_empty());
    }
}
