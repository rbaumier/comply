//! vitest-no-test-return-statement oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["test(", "it("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "test" && callee.name.as_str() != "it" {
            return;
        }
        let Some(cb) = call.arguments.get(1) else {
            return;
        };
        let (is_async, body_stmts) = match cb {
            Argument::ArrowFunctionExpression(a) => (a.r#async, &a.body.statements),
            Argument::FunctionExpression(f) => {
                let body = match &f.body {
                    Some(b) => b,
                    None => return,
                };
                (f.r#async, &body.statements)
            }
            _ => return,
        };
        // Async callbacks can legitimately return Promise — allow.
        if is_async {
            return;
        }
        // Find a `return <value>` in the top-level statements.
        for stmt in body_stmts.iter() {
            if let Statement::ReturnStatement(ret) = stmt
                && ret.argument.is_some()
            {
                // Allow call/new expressions — they may return a Promise,
                // which is the documented Vitest/Jest/Mocha async pattern.
                if let Some(arg) = &ret.argument {
                    if matches!(arg, Expression::CallExpression(_) | Expression::NewExpression(_)) {
                        return;
                    }
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`return <value>` in a test callback is silently discarded \
                              by vitest. Drop the return or mark the callback async."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.spec.ts")
    }

    #[test]
    fn allows_call_expression_return_regression_851() {
        // Regression for #851: supertest Promise chain must not be flagged.
        let d = run("it('x', () => { return request(server).get('/broadcast').expect(200, '2'); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_new_promise_return() {
        let d = run("it('x', () => { return new Promise(resolve => resolve()); });");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_literal_return() {
        let d = run("it('x', () => { return 42; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_identifier_return() {
        let d = run("it('x', () => { return someVariable; });");
        assert_eq!(d.len(), 1);
    }
}
