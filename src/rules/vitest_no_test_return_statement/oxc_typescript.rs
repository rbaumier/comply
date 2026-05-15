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
