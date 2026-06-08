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
    if !crate::rules::playwright::is_playwright_context(ctx) {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
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
