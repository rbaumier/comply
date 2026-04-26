//! playwright-no-eval — flag `page.$eval()` / `page.$$eval()` / `locator.$eval()`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(property) = callee.child_by_field_name("property") else { return };
    let prop_text = property.utf8_text(source).unwrap_or("");

    if prop_text != "$eval" && prop_text != "$$eval" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-eval".into(),
        message: format!(
            "`{prop_text}` runs arbitrary code against the DOM — prefer `locator()` + web-first assertions."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "login.test.ts")
    }

    #[test]
    fn flags_page_dollar_eval() {
        let d = run_ts("const t = await page.$eval('.btn', el => el.textContent);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-eval");
    }

    #[test]
    fn flags_page_dollar_dollar_eval() {
        let d = run_ts("const ts = await page.$$eval('li', els => els.map(e => e.textContent));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_locator_text_content() {
        let d = run_ts("const t = await page.locator('.btn').textContent();");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run_ts_with_path(
            "const t = await page.$eval('.btn', el => el.textContent);",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
