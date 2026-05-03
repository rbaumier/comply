//! comma-or-logical-or-case oxc backend — flag `case` clauses that use
//! comma or `||` instead of separate fall-through cases.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else {
            return;
        };

        for case in &switch.cases {
            let Some(test) = &case.test else {
                continue;
            };

            // Check for sequence expression: `case 1, 2:`
            let has_sequence = matches!(test, Expression::SequenceExpression(_));

            // Check for logical OR: `case 1 || 2:`
            let has_logical_or = if let Expression::LogicalExpression(logical) = test {
                matches!(
                    logical.operator,
                    oxc_ast::ast::LogicalOperator::Or
                )
            } else {
                false
            };

            if has_sequence || has_logical_or {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, case.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Switch `case` uses comma or `||` — use separate `case` clauses with fall-through instead.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
