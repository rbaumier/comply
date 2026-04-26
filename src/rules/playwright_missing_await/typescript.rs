//! playwright-missing-await — Playwright async calls without `await`.

use crate::diagnostic::{Diagnostic, Severity};

/// Known Playwright async method names (without the object prefix).
const ASYNC_METHODS: &[&str] = &[
    "goto", "click", "dblclick", "fill", "type", "press", "check",
    "uncheck", "selectOption", "setInputFiles", "waitForSelector",
    "waitForNavigation", "waitForLoadState", "waitForURL", "waitForEvent",
    "waitForFunction", "waitForTimeout", "waitForResponse", "waitForRequest",
    "screenshot", "pdf", "content", "title", "evaluate", "evaluateHandle",
    "reload", "goBack", "goForward", "close", "hover", "focus", "tap",
    "dragAndDrop", "setContent", "addInitScript", "route", "unroute",
    "exposeFunction", "emulateMedia", "setViewportSize", "setExtraHTTPHeaders",
    "dragTo", "scrollIntoViewIfNeeded", "selectText", "setChecked",
    "inputValue", "textContent", "innerText", "innerHTML", "getAttribute",
    "isVisible", "isHidden", "isEnabled", "isDisabled", "isChecked",
    "isEditable", "boundingBox", "waitFor", "clear",
    "newPage", "newContext", "clearCookies", "addCookies", "cookies",
    "storageState",
];

/// Playwright objects whose methods are async.
const PW_OBJECTS: &[&str] = &["page", "locator", "browser", "context", "frame"];

/// Known Playwright async expect matchers.
const ASYNC_EXPECT_METHODS: &[&str] = &[
    "toBeVisible", "toBeHidden", "toBeEnabled", "toBeDisabled",
    "toBeChecked", "toBeEditable", "toBeEmpty", "toBeFocused",
    "toBeAttached", "toBeInViewport", "toContainText", "toHaveAttribute",
    "toHaveClass", "toHaveCount", "toHaveCSS", "toHaveId",
    "toHaveJSProperty", "toHaveScreenshot", "toHaveText", "toHaveTitle",
    "toHaveURL", "toHaveValue", "toHaveValues", "toPass",
];

/// Walk ancestors to check if we're inside an `await_expression`.
fn is_inside_await(node: tree_sitter::Node) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind() == "await_expression" {
            return true;
        }
        // Don't walk past function boundaries.
        match p.kind() {
            "function_declaration" | "function" | "arrow_function"
            | "method_definition" => return false,
            _ => {}
        }
        cur = p.parent();
    }
    false
}

/// Check if the callee's object is a Playwright object.
fn is_pw_object(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    PW_OBJECTS.iter().any(|obj| {
        text == *obj || text.ends_with(&format!(".{obj}")) || text.ends_with(&format!("_{obj}"))
    })
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if is_inside_await(node) {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };

    // Check for page.method() / locator.method() patterns.
    if callee.kind() == "member_expression" {
        let Some(property) = callee.child_by_field_name("property") else { return };
        let method_name = property.utf8_text(source).unwrap_or("");

        // Check expect(...).toBeX pattern.
        let Some(object) = callee.child_by_field_name("object") else { return };
        if object.kind() == "call_expression" {
            let Some(expect_callee) = object.child_by_field_name("function") else { return };
            let expect_name = expect_callee.utf8_text(source).unwrap_or("");
            if expect_name == "expect"
                && ASYNC_EXPECT_METHODS.contains(&method_name)
            {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "playwright-missing-await".into(),
                    message: format!(
                        "`expect(...).{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
                return;
            }
        }

        // Check for .not.toBeX pattern (expect(...).not.toBeVisible())
        if object.kind() == "member_expression" {
            let Some(not_prop) = object.child_by_field_name("property") else { return };
            if not_prop.utf8_text(source).unwrap_or("") == "not" {
                let Some(inner_obj) = object.child_by_field_name("object") else { return };
                if inner_obj.kind() == "call_expression" {
                    let Some(expect_callee) = inner_obj.child_by_field_name("function") else { return };
                    if expect_callee.utf8_text(source).unwrap_or("") == "expect"
                        && ASYNC_EXPECT_METHODS.contains(&method_name)
                    {
                        let pos = node.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "playwright-missing-await".into(),
                            message: format!(
                                "`expect(...).not.{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        return;
                    }
                }
            }
        }

        // Check for playwright object method calls.
        if ASYNC_METHODS.contains(&method_name) && is_pw_object(object, source) {
            let obj_text = object.utf8_text(source).unwrap_or("?");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "playwright-missing-await".into(),
                message: format!(
                    "`{obj_text}.{method_name}` is an async Playwright method — add `await` to prevent flaky tests."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_missing_await_on_page_click() {
        let d = run_ts("page.click('#button');", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("page.click"));
    }

    #[test]
    fn flags_missing_await_on_expect() {
        let d = run_ts("expect(locator).toBeVisible();", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeVisible"));
    }

    #[test]
    fn allows_awaited_calls() {
        let source = r#"
await page.click('#button');
await expect(locator).toBeVisible();
"#;
        assert!(run_ts(source, &Check).is_empty());
    }

    #[test]
    fn flags_locator_fill() {
        let d = run_ts("locator.fill('hello');", &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("fill"));
    }
}
