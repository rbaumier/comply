//! prefer-immediate-return OXC backend — flag `const x = expr; return x;`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::FunctionBody]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::FunctionBody(body) = node.kind() else {
            return;
        };

        let stmts = &body.statements;
        if stmts.len() < 2 {
            return;
        }

        for window in stmts.windows(2) {
            let decl_stmt = &window[0];
            let ret_stmt = &window[1];

            // First must be a variable declaration
            let Statement::VariableDeclaration(var_decl) = decl_stmt else {
                continue;
            };

            // Only single declarators
            if var_decl.declarations.len() != 1 {
                continue;
            }
            let declarator = &var_decl.declarations[0];

            // Must be a simple identifier (not destructuring)
            let oxc_ast::ast::BindingPattern::BindingIdentifier(ref binding) = declarator.id
            else {
                continue;
            };
            let var_name = binding.name.as_str();

            // Next must be a return statement
            let Statement::ReturnStatement(ret) = ret_stmt else {
                continue;
            };

            // Return value must be an identifier matching the declared name
            let Some(Expression::Identifier(ret_id)) = &ret.argument else {
                continue;
            };
            if ret_id.name.as_str() != var_name {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, var_decl.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Variable `{var_name}` is assigned and immediately \
                     returned — return the expression directly."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
