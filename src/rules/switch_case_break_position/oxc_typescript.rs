use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn keyword_for<'a>(stmt: &Statement<'a>) -> &'static str {
    match stmt {
        Statement::BreakStatement(_) => "break",
        Statement::ReturnStatement(_) => "return",
        Statement::ContinueStatement(_) => "continue",
        Statement::ThrowStatement(_) => "throw",
        _ => "unknown",
    }
}

fn is_terminator(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::BreakStatement(_)
            | Statement::ReturnStatement(_)
            | Statement::ContinueStatement(_)
            | Statement::ThrowStatement(_)
    )
}

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
        let AstKind::SwitchCase(case) = node.kind() else { return };

        let body = &case.consequent;
        // Need at least 2 statements: a block + a terminator after it.
        if body.len() < 2 {
            return;
        }

        let last = &body[body.len() - 1];
        if !is_terminator(last) {
            return;
        }

        // Everything before the terminator should be exactly one block statement.
        let before_terminator = &body[..body.len() - 1];
        if before_terminator.len() != 1 {
            return;
        }
        if !matches!(&before_terminator[0], Statement::BlockStatement(_)) {
            return;
        }

        let keyword = keyword_for(last);
        let span = last.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Move `{keyword}` inside the block statement."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
