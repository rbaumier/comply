//! OxcCheck backend for playwright-no-duplicate-hooks — disallow
//! duplicate setup/teardown hooks in describe blocks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];
const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
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
        let program = semantic.nodes().program();
        check_statements(&program.body, ctx, &mut diagnostics);
        diagnostics
    }
}

fn check_statements(
    stmts: &[Statement<'_>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut hook_counts: FxHashMap<&str, usize> = FxHashMap::default();

    for stmt in stmts {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };

        // If this is a describe call, recurse into its callback body.
        if is_describe_call(call) {
            if let Some(last_arg) = call.arguments.last() {
                match last_arg {
                    Argument::ArrowFunctionExpression(arrow) => {
                        check_statements(&arrow.body.statements, ctx, diagnostics);
                    }
                    Argument::FunctionExpression(func) => {
                        if let Some(ref body) = func.body {
                            check_statements(&body.statements, ctx, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        // Check if this is a hook call.
        if let Some(name) = get_hook_name(call) {
            let entry = hook_counts.entry(name).or_insert(0);
            *entry += 1;
            if *entry > 1 {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Duplicate {name} in describe block."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn is_describe_call(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    match &call.callee {
        Expression::Identifier(id) => id.name.as_str() == "describe",
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "describe"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn get_hook_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    let name = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return None,
    };
    if HOOKS.contains(&name) {
        Some(name)
    } else {
        None
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
    fn flags_duplicate_before_each() {
        let src = "\
describe('suite', () => {
  beforeEach(() => {});
  beforeEach(() => {});
  test('a', () => {});
});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-duplicate-hooks");
    }

    #[test]
    fn allows_different_hooks() {
        let src = "\
describe('suite', () => {
  beforeEach(() => {});
  afterEach(() => {});
  test('a', () => {});
});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
