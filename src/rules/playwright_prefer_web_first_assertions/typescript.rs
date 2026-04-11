//! playwright-prefer-web-first-assertions AST backend — flag `expect(await locator.isVisible())` etc.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Playwright locator methods that have web-first assertion equivalents.
const LOCATOR_METHODS: &[&str] = &[
    "isVisible",
    "isHidden",
    "isEnabled",
    "isDisabled",
    "isChecked",
    "isEditable",
    "textContent",
    "innerText",
    "innerHTML",
    "getAttribute",
    "inputValue",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    // Check if this is `expect(...)`.
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "expect" {
        return;
    }

    // First argument should be an `await_expression`.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor)
        .find(|c| !matches!(c.kind(), "(" | ")" | ","));

    let Some(arg) = first_arg else { return };
    if arg.kind() != "await_expression" {
        return;
    }

    // The awaited expression should be a call_expression with a locator method.
    let awaited = arg.child(1); // await <expr>
    let Some(awaited_expr) = awaited else { return };
    if awaited_expr.kind() != "call_expression" {
        return;
    }

    let Some(inner_callee) = awaited_expr.child_by_field_name("function") else { return };
    if inner_callee.kind() != "member_expression" {
        return;
    }

    let Some(method) = inner_callee.child_by_field_name("property") else { return };
    let method_name = method.utf8_text(source).unwrap_or("");
    if !LOCATOR_METHODS.contains(&method_name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-web-first-assertions".into(),
        message: "Use web-first assertions (`toBeVisible`, \
                  `toBeEnabled`, etc.) instead of asserting on \
                  awaited locator methods — they auto-retry."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, "login.test.ts")
    }

    #[test]
    fn flags_is_visible_assertion() {
        let d = run_on("expect(await page.locator('#btn').isVisible()).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-web-first-assertions");
    }

    #[test]
    fn flags_text_content_assertion() {
        let d = run_on("expect(await el.textContent()).toContain('Hello');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_web_first_assertion() {
        let d = run_on("await expect(page.locator('#btn')).toBeVisible();");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_expect_await_with_non_locator() {
        let d = run_on("expect(await fetch('/api')).toBeDefined();");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_ts_with_path(
            "expect(await el.isVisible()).toBe(true);",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
