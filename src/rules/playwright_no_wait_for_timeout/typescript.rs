//! playwright-no-wait-for-timeout — disallow `page.waitForTimeout()`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["call_expression"] prefilter = ["waitForTimeout"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "waitForTimeout" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-wait-for-timeout".into(),
        message: "Unexpected use of page.waitForTimeout(); prefer web-first assertions.".into(),
        severity: Severity::Warning,
        span: None,
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
    fn flags_wait_for_timeout() {
        let d = run_ts("await page.waitForTimeout(1000);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-wait-for-timeout");
    }

    #[test]
    fn allows_wait_for_selector() {
        let d = run_ts("await page.waitForSelector('.btn');");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_web_first_assertion() {
        let d = run_ts("await expect(page.locator('.btn')).toBeVisible();");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_nested_wait_for_timeout() {
        let d = run_ts("test('x', async ({ page }) => { await page.waitForTimeout(500); });");
        assert_eq!(d.len(), 1);
    }
}
