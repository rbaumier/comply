//! prefer-set-size OXC backend — flag `[...set].length` and `Array.from(set).length`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StaticMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StaticMemberExpression(member) = node.kind() else { return };

        if member.property.name != "length" {
            return;
        }

        let obj = &member.object;
        let is_spread_array = matches!(obj, Expression::ArrayExpression(arr) if {
            let non_elision = arr.elements.iter().filter(|e| !e.is_elision()).count();
            non_elision == 1 && arr.elements.iter().any(|e| e.is_spread())
        });

        let is_array_from = matches!(obj, Expression::CallExpression(call) if {
            matches!(&call.callee, Expression::StaticMemberExpression(m)
                if m.property.name == "from"
                && matches!(&m.object, Expression::Identifier(id) if id.name == "Array")
            )
        });

        if !is_spread_array && !is_array_from {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Set#size` instead of `[...set].length` or `Array.from(set).length`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
