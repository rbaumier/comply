//! switch-case-braces OXC backend — flag `case` clauses whose body is not
//! wrapped in a block `{ }`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchCase]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchCase(case) = node.kind() else {
            return;
        };

        let stmts = &case.consequent;

        // Fall-through case (no body)
        if stmts.is_empty() {
            return;
        }

        // Already wrapped in a single block statement
        if stmts.len() == 1 && matches!(stmts[0], Statement::BlockStatement(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, case.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Missing braces in `case` clause \u{2014} wrap the body in `{ }` \
                      to avoid scope leaking."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
