//! playwright-prefer-hooks-on-top OxcCheck backend — hooks should come before
//! test cases within each describe block.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];
const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

/// Get the callee name of a call expression. For `test(...)` returns "test",
/// for `test.only(...)` returns "test".
fn call_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>, _source: &str) -> Option<&'a str> {
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(mem) => {
            // For test.only, test.skip etc, the object is the test fn name
            if let Expression::Identifier(id) = &mem.object {
                Some(id.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn check_statements(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen_test = false;
    for stmt in stmts {
        let Statement::ExpressionStatement(expr_stmt) = stmt else { continue };
        let Expression::CallExpression(call) = &expr_stmt.expression else { continue };

        if let Some(name) = call_name(call, ctx.source) {
            if TEST_FNS.contains(&name) {
                seen_test = true;
            } else if HOOKS.contains(&name) && seen_test {
                let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Hooks should come before test cases.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }

            // Recurse into describe callbacks.
            let is_describe = match &call.callee {
                Expression::Identifier(id) => id.name.as_str() == "describe",
                Expression::StaticMemberExpression(mem) => {
                    if let Expression::Identifier(id) = &mem.object {
                        id.name.as_str() == "describe"
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if is_describe
                && let Some(last_arg) = call.arguments.last() {
                    if let oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) = last_arg {
                        check_statements(&arrow.body.statements, ctx, diagnostics);
                    } else if let oxc_ast::ast::Argument::FunctionExpression(func) = last_arg
                        && let Some(body) = &func.body {
                            check_statements(&body.statements, ctx, diagnostics);
                        }
                }
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                check_statements(&program.body, ctx, &mut diagnostics);
                break;
            }
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
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
    }

    #[test]
    fn flags_hook_after_test() {
        let src = "\
test('a', () => {});
beforeEach(() => {});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-hooks-on-top");
    }

    #[test]
    fn allows_hooks_before_tests() {
        let src = "\
beforeEach(() => {});
test('a', () => {});
test('b', () => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
