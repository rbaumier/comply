//! Flags positive literal retry counts in tests.
//! Config files: any `retry`/`retries` property.
//! Test files: test-runner call options, `this.retries`, `jest.retryTimes`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

const CONFIG_STEMS: &[&str] = &[
    "vitest.config",
    "vite.config",
    "vitest.workspace",
    "playwright.config",
];

const TEST_CALLEE_ROOTS: &[&str] = &["it", "test", "describe", "suite", "bench"];

fn is_config_file(ctx: &CheckCtx) -> bool {
    ctx.path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| CONFIG_STEMS.contains(&stem))
}

fn is_positive_count(expr: &Expression) -> bool {
    matches!(expr, Expression::NumericLiteral(n) if n.value > 0.0)
}

/// Callee rooted at a test function, or `vi.setConfig`.
fn is_test_runner_call(call: &CallExpression) -> bool {
    if let Expression::StaticMemberExpression(member) = &call.callee
        && member.property.name == "setConfig"
        && matches!(&member.object, Expression::Identifier(id) if id.name == "vi")
    {
        return true;
    }
    let mut current = &call.callee;
    loop {
        current = match current {
            Expression::Identifier(id) => return TEST_CALLEE_ROOTS.contains(&id.name.as_str()),
            Expression::StaticMemberExpression(m) => &m.object,
            Expression::ComputedMemberExpression(m) => &m.object,
            Expression::CallExpression(c) => &c.callee,
            _ => return false,
        };
    }
}

fn is_test_runner_option(node: &oxc_semantic::AstNode, semantic: &oxc_semantic::Semantic) -> bool {
    let nodes = semantic.nodes();
    let object = nodes.parent_node(node.id());
    matches!(
        nodes.parent_node(object.id()).kind(),
        AstKind::CallExpression(call) if is_test_runner_call(call)
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["retry", "retries"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let in_test_dir = ctx.file.path_segments.in_test_dir;
        let start = match node.kind() {
            AstKind::ObjectProperty(prop) => {
                let is_retry_key = prop
                    .key
                    .static_name()
                    .is_some_and(|name| name == "retry" || name == "retries");
                if !is_retry_key || !is_positive_count(&prop.value) {
                    return;
                }
                let is_reported =
                    is_config_file(ctx) || (in_test_dir && is_test_runner_option(node, semantic));
                if !is_reported {
                    return;
                }
                prop.span.start
            }
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let is_retry_call = match member.property.name.as_str() {
                    "retries" => matches!(member.object, Expression::ThisExpression(_)),
                    "retryTimes" => {
                        matches!(&member.object, Expression::Identifier(id) if id.name == "jest")
                    }
                    _ => false,
                };
                let has_positive_count = call
                    .arguments
                    .first()
                    .and_then(Argument::as_expression)
                    .is_some_and(is_positive_count);
                if !in_test_dir || !is_retry_call || !has_positive_count {
                    return;
                }
                call.span.start
            }
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Test retries hide flakiness \u{2014} fix the test, or retry only in CI with `process.env.CI ? n : 0`.".into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn count(src: &str, path: &str) -> usize {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path).len()
    }

    #[test]
    fn flags_retry_option_on_it_call() {
        assert_eq!(count("it(\"x\", { retry: 2 }, () => {})", "a.test.ts"), 1);
    }

    #[test]
    fn flags_retry_option_on_describe_each() {
        assert_eq!(
            count(
                "describe.each(rows)(\"x\", { retry: 1 }, () => {})",
                "a.test.ts"
            ),
            1
        );
    }

    #[test]
    fn flags_retry_option_on_test_concurrent() {
        assert_eq!(
            count(
                "test.concurrent(\"x\", { retry: 2 }, async () => {})",
                "a.test.ts"
            ),
            1
        );
    }

    #[test]
    fn allows_retry_zero_on_test_call() {
        assert_eq!(count("it(\"x\", { retry: 0 }, () => {})", "a.test.ts"), 0);
    }

    #[test]
    fn flags_vi_set_config_retry() {
        assert_eq!(count("vi.setConfig({ retry: 3 })", "a.test.ts"), 1);
    }

    #[test]
    fn flags_vitest_config_test_retry() {
        assert_eq!(
            count(
                "export default defineConfig({ test: { retry: 2 } })",
                "vitest.config.ts"
            ),
            1
        );
    }

    #[test]
    fn flags_vitest_config_project_retry() {
        let src = "export default defineConfig({ test: { projects: [{ test: { retry: 1 } }] } })";
        assert_eq!(count(src, "vitest.config.mts"), 1);
    }

    #[test]
    fn allows_test_retry_object_outside_vitest_config() {
        assert_eq!(count("const o = { test: { retry: 2 } }", "src/app.ts"), 0);
    }

    #[test]
    fn flags_jest_retry_times() {
        assert_eq!(count("jest.retryTimes(3)", "a.test.ts"), 1);
    }

    #[test]
    fn allows_jest_retry_times_zero() {
        assert_eq!(count("jest.retryTimes(0)", "a.test.ts"), 0);
    }

    #[test]
    fn flags_playwright_describe_configure_retries() {
        assert_eq!(
            count("test.describe.configure({ retries: 2 })", "e2e/a.spec.ts"),
            1
        );
    }

    #[test]
    fn flags_mocha_this_retries() {
        assert_eq!(
            count(
                "describe(\"x\", function () { this.retries(3) })",
                "test/a.js"
            ),
            1
        );
    }

    #[test]
    fn allows_this_retries_outside_test_dir() {
        assert_eq!(
            count("class C { run() { this.retries(3) } }", "src/client.ts"),
            0
        );
    }

    #[test]
    fn allows_query_client_retry_false_in_test() {
        let src = "new QueryClient({ defaultOptions: { queries: { retry: false } } })";
        assert_eq!(count(src, "a.test.ts"), 0);
    }

    #[test]
    fn allows_retry_option_on_non_test_call() {
        assert_eq!(
            count("useQuery({ queryKey: [\"k\"], retry: 3 })", "src/hooks.ts"),
            0
        );
    }

    #[test]
    fn allows_playwright_ci_conditional_retries() {
        let src = "export default defineConfig({ retries: process.env.CI ? 2 : 0 })";
        assert_eq!(count(src, "playwright.config.ts"), 0);
    }

    #[test]
    fn flags_playwright_config_literal_retries() {
        assert_eq!(
            count(
                "export default defineConfig({ retries: 2 })",
                "playwright.config.ts"
            ),
            1
        );
    }

    #[test]
    fn allows_retry_option_on_non_test_call_in_test_file() {
        assert_eq!(count("withRetry(fn, { retries: 3 })", "retry.test.ts"), 0);
    }

    #[test]
    fn allows_retries_method_on_non_this_receiver_in_test_file() {
        assert_eq!(count("policy.retries(2)", "policy.test.ts"), 0);
    }

    #[test]
    fn allows_retries_outside_playwright_config() {
        assert_eq!(count("got.extend({ retries: 2 })", "src/http.ts"), 0);
    }
}
