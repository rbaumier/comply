//! OXC backend for prefer-spy-on — detect `obj.method = vi.fn()` /
//! `obj.method = jest.fn()` and suggest `spyOn` instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["vi.fn", "jest.fn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // LHS must be a member expression (obj.method).
        let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(left) = &assign.left else {
            return;
        };

        // RHS must be a call expression.
        let oxc_ast::ast::Expression::CallExpression(call) = &assign.right else {
            return;
        };

        // Callee must be `vi.fn` or `jest.fn`.
        let oxc_ast::ast::Expression::StaticMemberExpression(callee) = &call.callee else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(callee_obj) = &callee.object else {
            return;
        };
        let framework = if callee_obj.name == "vi" && callee.property.name == "fn" {
            "vi"
        } else if callee_obj.name == "jest" && callee.property.name == "fn" {
            "jest"
        } else {
            return;
        };

        let obj_span = left.object.span();
        let obj_text = &ctx.source[obj_span.start as usize..obj_span.end as usize];
        let prop_text = left.property.name.as_str();

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Reassigning `{obj_text}.{prop_text}` with `{framework}.fn()` replaces the \
                 original implementation — use `{framework}.spyOn({obj_text}, '{prop_text}')` instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
