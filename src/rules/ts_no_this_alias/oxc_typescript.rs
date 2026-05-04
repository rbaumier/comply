//! OXC backend for ts-no-this-alias.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator, AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                if !matches!(init, Expression::ThisExpression(_)) {
                    return;
                }
                // Allow destructuring: `const { a } = this`
                let BindingPattern::BindingIdentifier(id) = &decl.id else {
                    return;
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, id.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Unexpected aliasing of `this` to a local variable.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::AssignmentExpression(assign) => {
                if !matches!(&assign.right, Expression::ThisExpression(_)) {
                    return;
                }
                let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                    return;
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, id.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Unexpected aliasing of `this` to a local variable.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
