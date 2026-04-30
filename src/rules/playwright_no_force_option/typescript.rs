//! playwright-no-force-option — flag `{ force: true }` on Playwright actions.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Playwright actions that accept a `force` option.
const FORCE_ACTIONS: &[&str] = &[
    "click",
    "fill",
    "hover",
    "check",
    "uncheck",
    "selectOption",
    "dblclick",
    "tap",
    "press",
    "dragTo",
];

/// Check if a node is a `force: true` property assignment.
fn is_force_true(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "pair" {
        return false;
    }
    let Some(key) = node.child_by_field_name("key") else {
        return false;
    };
    let Some(value) = node.child_by_field_name("value") else {
        return false;
    };
    key.utf8_text(source).unwrap_or("") == "force"
        && value.utf8_text(source).unwrap_or("") == "true"
}

/// Walk descendants to find a `force: true` pair in an object literal.
fn has_force_true(node: tree_sitter::Node, source: &[u8]) -> bool {
    if is_force_true(node, source) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_force_true(child, source) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !source.windows(16).any(|w| w == b"@playwright/test") {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(property) = callee.child_by_field_name("property") else { return };
    let method_name = property.utf8_text(source).unwrap_or("");

    if !FORCE_ACTIONS.contains(&method_name) {
        return;
    }

    // Check arguments for `force: true`.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !has_force_true(args, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-force-option".into(),
        message: "`force: true` bypasses actionability checks — fix the underlying page state instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let full = format!("{source}\n// @playwright/test");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(&full, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), &full), &tree)
    }

    #[test]
    fn flags_force_true_on_click() {
        let d = run(
            "login.test.ts",
            "await page.click('#btn', { force: true });",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-force-option");
    }

    #[test]
    fn flags_force_true_on_fill() {
        let d = run(
            "form.spec.ts",
            "await input.fill('hello', { force: true });",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_click_without_force() {
        let d = run("login.test.ts", "await page.click('#btn');");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_force_false() {
        let d = run(
            "login.test.ts",
            "await page.click('#btn', { force: false });",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run("helpers.ts", "await page.click('#btn', { force: true });");
        assert!(d.is_empty());
    }
}
