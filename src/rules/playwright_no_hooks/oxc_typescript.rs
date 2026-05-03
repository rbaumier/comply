//! playwright-no-hooks OxcCheck backend — disallow setup and teardown hooks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !ctx.project.has_framework("playwright") {
            return;
        }
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::imports_playwright_test(ctx.source) {
            return;
        }

        let name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(mem) => mem.property.name.as_str(),
            _ => return,
        };

        if !HOOKS.contains(&name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Unexpected '{name}' hook."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path_and_framework(
            source,
            &Check,
            "app.test.ts",
            "playwright",
        )
    }

    #[test]
    fn flags_before_each() {
        let d = run_ts(
            r#"import { test } from "@playwright/test";
beforeEach(() => { setup(); });"#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-hooks");
    }

    #[test]
    fn flags_after_all() {
        let d = run_ts(
            r#"import { test } from "@playwright/test";
afterAll(() => { cleanup(); });"#,
        );
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
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "app.test.ts");
        assert!(
            d.is_empty(),
            "framework-scoped rule must be silent without detected Playwright"
        );
    }
}
