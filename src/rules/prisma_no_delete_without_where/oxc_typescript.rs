//! prisma-no-delete-without-where oxc backend — flag `deleteMany()` without `where`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind};
use std::sync::Arc;

fn is_prisma_file(source: &str) -> bool {
    source.contains("@prisma/client")
        || source.contains("PrismaClient")
        || source.contains("prisma.")
}

fn object_has_where(expr: &Expression) -> bool {
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        if let ObjectPropertyKind::ObjectProperty(p) = prop {
            p.key.name().is_some_and(|n| n == "where")
        } else {
            false
        }
    })
}

pub struct Check;

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
        if !is_prisma_file(ctx.source) {
            return;
        }

        // Callee must be `*.deleteMany`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "deleteMany" {
            return;
        }

        // No arguments at all — deletes every row.
        if call.arguments.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`deleteMany()` with no arguments deletes every row in the table."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Check if any object argument has a `where` key.
        for arg in &call.arguments {
            let Some(expr) = arg.as_expression() else { continue };
            if let Expression::ObjectExpression(_) = expr {
                if !object_has_where(expr) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "`deleteMany()` without `where` deletes every row in the table."
                            .into(),
                        severity: Severity::Error,
                        span: None,
                    });
                    return;
                }
            }
        }
    }
}
