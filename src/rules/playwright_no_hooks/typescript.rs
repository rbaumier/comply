//! playwright-no-hooks — disallow setup and teardown hooks.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("playwright") { return; }
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::imports_playwright_test(ctx.source) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    let name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            if let Some(prop) = callee.child_by_field_name("property") {
                prop.utf8_text(source).unwrap_or("")
            } else {
                return;
            }
        }
        _ => return,
    };

    if !HOOKS.contains(&name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-hooks".into(),
        message: format!("Unexpected '{name}' hook."),
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
    use crate::project::ProjectCtx;
    use std::path::Path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        let project = ProjectCtx::for_test_with_framework("playwright");
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, Path::new("app.test.ts"), &project, crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_before_each() {
        let d = run_ts(r#"import { test } from "@playwright/test";
beforeEach(() => { setup(); });"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-hooks");
    }

    #[test]
    fn flags_after_all() {
        let d = run_ts(r#"import { test } from "@playwright/test";
afterAll(() => { cleanup(); });"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_hook() {
        let d = run_ts("test('works', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_when_file_imports_playwright() {
        let src = r#"
import { test, expect } from "@playwright/test";
test.beforeEach(async ({ page }) => { await page.goto("/"); });
"#;
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_vitest_test_file_with_before_each() {
        let src = r#"
import { describe, it, beforeEach, afterEach, expect } from "vitest";
beforeEach(() => { reset(); });
afterEach(() => { cleanup(); });
describe("x", () => { it("works", () => { expect(1).toBe(1); }); });
"#;
        let d = run_ts(src);
        assert!(d.is_empty(), "vitest hooks must not be flagged: {d:?}");
    }

    #[test]
    fn ignores_jest_test_file_with_before_each() {
        let src = r#"
import { describe, it, beforeEach, expect } from "@jest/globals";
beforeEach(() => { reset(); });
"#;
        let d = run_ts(src);
        assert!(d.is_empty(), "jest hooks must not be flagged: {d:?}");
    }

    #[test]
    fn ignores_test_file_with_no_test_framework_import() {
        let src = r#"
beforeEach(() => { reset(); });
"#;
        let d = run_ts(src);
        assert!(
            d.is_empty(),
            "must not flag when file does not import @playwright/test: {d:?}"
        );
    }

    #[test]
    fn ignores_playwright_import_when_project_is_not_playwright() {
        let src = r#"
import { test } from "@playwright/test";
beforeEach(() => { reset(); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "app.test.ts");
        assert!(
            d.is_empty(),
            "framework-scoped rule must be silent without detected Playwright"
        );
    }
}
