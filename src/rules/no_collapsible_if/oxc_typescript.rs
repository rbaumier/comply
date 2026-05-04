//! no-collapsible-if oxc backend — flag `if (a) { if (b) {} }` that should be
//! `if (a && b) {}`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else { return };

        // The outer if must NOT have an else clause.
        if stmt.alternate.is_some() {
            return;
        }

        // Get the consequence (body) of the outer if — must be a block.
        let Statement::BlockStatement(block) = &stmt.consequent else { return };

        // The body must contain exactly one statement.
        if block.body.len() != 1 {
            return;
        }

        // That single child must be an if_statement.
        let Statement::IfStatement(inner) = &block.body[0] else { return };

        // The inner if must also NOT have an else clause to be collapsible.
        if inner.alternate.is_some() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Nested `if` without `else` can be merged into a single `if (a && b)`.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
