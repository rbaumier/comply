//! no-this-assignment OXC backend — flag `const self = this` and `self = this`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in decl.declarations.iter() {
                    let Some(init) = &declarator.init else { continue };
                    if !matches!(init, Expression::ThisExpression(_)) {
                        continue;
                    }
                    let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                        continue;
                    };
                    let var_name = id.name.as_str();
                    let (line, column) = byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                if !matches!(&assign.right, Expression::ThisExpression(_)) {
                    return;
                }
                let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                    return;
                };
                let var_name = id.name.as_str();
                let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Do not assign `this` to `{var_name}`. Use an arrow function instead."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
