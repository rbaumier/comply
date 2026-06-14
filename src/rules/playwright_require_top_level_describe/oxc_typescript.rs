//! playwright-require-top-level-describe oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

const LIFECYCLE_HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

fn is_playwright_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@playwright/test") || crate::oxc_helpers::source_contains(source, "playwright/test")
}

fn is_bare_test_call(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    id.name.as_str() == "test"
}

/// True for a `<id>.beforeAll(...)` / `.beforeEach` / `.afterAll` /
/// `.afterEach` member call. The object identifier is not constrained, so
/// an aliased `test` binding (e.g. `const test = playwrightTest.extend(...)`)
/// is matched the same as the bare `test`.
fn is_lifecycle_hook_call(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    LIFECYCLE_HOOKS.contains(&member.property.name.as_str())
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["test("])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_playwright_file(ctx.source) {
            return Vec::new();
        }

        let body = &semantic.nodes().program().body;

        // Top-level lifecycle hooks prove the file's tests are an organized
        // group (one spec file per feature, the file name is the grouping),
        // so flat top-level `test(...)` calls are intentional — skip them.
        let has_top_level_hook = body.iter().any(|stmt| {
            let Statement::ExpressionStatement(expr_stmt) = stmt else {
                return false;
            };
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                return false;
            };
            is_lifecycle_hook_call(call)
        });
        if has_top_level_hook {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for stmt in body {
            let Statement::ExpressionStatement(expr_stmt) = stmt else {
                continue;
            };
            let Expression::CallExpression(call) = &expr_stmt.expression else {
                continue;
            };
            if !is_bare_test_call(call) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Top-level `test(...)` — wrap in `test.describe(\"<feature>\", \
                          () => { ... })` so reports group related cases."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "trace.spec.ts")
    }

    #[test]
    fn flags_top_level_test_without_describe_or_hooks() {
        let src = "\
test('a', async () => {});
test('b', async () => {});";
        let d = run_ts(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.rule_id == "playwright-require-top-level-describe"));
    }

    #[test]
    fn allows_top_level_test_inside_describe() {
        let src = "\
test.describe('suite', () => {
  test('a', async () => {});
});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_tests_when_top_level_lifecycle_hook_present() {
        let src = "\
test.beforeAll(async () => {});

test('a', async () => {});
test('b', async () => {});
test('c', async () => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_tests_with_aliased_test_binding_and_hook() {
        // Reproduces playwright's tests/library/trace-viewer.spec.ts shape:
        // an aliased `test` binding plus a top-level lifecycle hook.
        let src = "\
const test = playwrightTest.extend(traceViewerFixtures);

test.beforeAll(async function recordTrace({ browser }) {});

test('should show empty trace viewer', async ({ showTraceViewer }) => {});
test('should open two trace viewers', async ({ showTraceViewer }) => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }

    #[test]
    fn allows_flat_tests_with_before_each_hook() {
        let src = "\
test.beforeEach(async () => {});

test('a', async () => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
