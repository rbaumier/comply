//! no-import-module-exports — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut has_import = false;
        let mut module_exports_spans: Vec<oxc_span::Span> = Vec::new();

        for stmt in &semantic.nodes().program().body {
            if matches!(stmt, Statement::ImportDeclaration(_)) {
                has_import = true;
                continue;
            }

            // expression_statement containing module.exports = ... or exports.foo = ...
            if let Statement::ExpressionStatement(expr_stmt) = stmt
                && let Expression::AssignmentExpression(assign) = &expr_stmt.expression {
                    let left_text = match &assign.left {
                        AssignmentTarget::StaticMemberExpression(member) => {
                            &ctx.source
                                [member.span().start as usize..member.span().end as usize]
                        }
                        AssignmentTarget::ComputedMemberExpression(member) => {
                            &ctx.source
                                [member.span().start as usize..member.span().end as usize]
                        }
                        _ => continue,
                    };
                    if left_text.starts_with("module.exports")
                        || left_text.starts_with("exports.")
                    {
                        module_exports_spans.push(expr_stmt.span);
                    }
                }
        }

        if !has_import {
            return Vec::new();
        }

        module_exports_spans
            .iter()
            .map(|span| {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Cannot use `module.exports`/`exports` in a module that uses `import` declarations — pick one module system.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}
