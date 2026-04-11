//! playwright-no-useless-await — flag unnecessary `await` on sync Playwright methods.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Locator methods that are synchronous (return Locator, not Promise).
const SYNC_LOCATOR_METHODS: &[&str] = &[
    "and", "first", "getByAltText", "getByLabel", "getByPlaceholder",
    "getByRole", "getByTestId", "getByText", "getByTitle", "last",
    "locator", "nth", "or",
];

/// Page/frame methods that are synchronous.
const SYNC_PAGE_METHODS: &[&str] = &[
    "frameLocator", "isClosed", "url", "viewportSize",
];

/// Sync expect matchers (non-web-first).
const SYNC_MATCHERS: &[&str] = &[
    "toBe", "toBeCloseTo", "toBeDefined", "toBeFalsy", "toBeGreaterThan",
    "toBeGreaterThanOrEqual", "toBeInstanceOf", "toBeLessThan",
    "toBeLessThanOrEqual", "toBeNaN", "toBeNull", "toBeTruthy",
    "toBeUndefined", "toContain", "toContainEqual", "toEqual",
    "toHaveLength", "toHaveProperty", "toMatch", "toMatchObject",
    "toStrictEqual", "toThrow", "toThrowError",
];

fn get_method_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let callee = node.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    prop.utf8_text(source).ok()
}

/// Check if a call expression is `expect(…).matcher(…)` with a sync matcher.
fn is_sync_expect_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    let matcher = prop.utf8_text(source).unwrap_or("");
    if !SYNC_MATCHERS.contains(&matcher) {
        return false;
    }
    // The object should be an expect() call or expect().not
    let Some(obj) = callee.child_by_field_name("object") else { return false };
    contains_expect_root(obj, source)
}

fn contains_expect_root(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" => {
            if let Some(fn_node) = node.child_by_field_name("function")
                && fn_node.kind() == "identifier" {
                    return fn_node.utf8_text(source).unwrap_or("") == "expect";
                }
            false
        }
        "member_expression" => {
            if let Some(obj) = node.child_by_field_name("object") {
                contains_expect_root(obj, source)
            } else {
                false
            }
        }
        _ => false,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "await_expression" {
        return;
    }

    let Some(child) = node.named_child(0) else { return };
    if child.kind() != "call_expression" {
        return;
    }

    let is_useless = if let Some(method) = get_method_name(child, source) {
        SYNC_LOCATOR_METHODS.contains(&method) || SYNC_PAGE_METHODS.contains(&method)
    } else {
        false
    } || is_sync_expect_chain(child, source);

    if !is_useless {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-useless-await".into(),
        message: "Unnecessary await expression. This method does not return a Promise.".into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_await_locator() {
        let d = run_ts("const el = await page.locator('.btn');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-useless-await");
    }

    #[test]
    fn flags_await_sync_expect() {
        let d = run_ts("await expect(1).toBe(1);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_await_click() {
        let d = run_ts("await page.click('.btn');");
        assert!(d.is_empty());
    }
}
