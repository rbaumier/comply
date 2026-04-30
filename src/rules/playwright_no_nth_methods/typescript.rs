//! playwright-no-nth-methods — disallow `.first()`, `.last()`, `.nth()`.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const NTH_METHODS: &[&str] = &["first", "last", "nth"];

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
    let method = property.utf8_text(source).unwrap_or("");

    if !NTH_METHODS.contains(&method) {
        return;
    }

    let pos = property.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-nth-methods".into(),
        message: format!("Unexpected use of {method}()."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }

    #[test]
    fn flags_first() {
        let d = run_ts("const el = page.locator('.btn').first();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-nth-methods");
    }

    #[test]
    fn flags_nth() {
        let d = run_ts("const el = page.locator('.btn').nth(2);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_other_methods() {
        let d = run_ts("const el = page.locator('.btn').click();");
        assert!(d.is_empty());
    }
}
