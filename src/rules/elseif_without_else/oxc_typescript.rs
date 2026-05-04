//! elseif-without-else — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::IfStatement(if_stmt) = node.kind() else {
            return;
        };

        // Only process top-level if statements. If this if_statement is the
        // alternate of a parent if, skip — we process the chain from its root.
        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id() {
            let parent_kind = nodes.get_node(parent_id).kind();
            if matches!(parent_kind, oxc_ast::AstKind::IfStatement(_)) {
                // Check if we are in the alternate branch of the parent if.
                if let oxc_ast::AstKind::IfStatement(parent_if) = parent_kind {
                    if let Some(alt) = &parent_if.alternate {
                        if let Statement::IfStatement(alt_if) = alt {
                            if std::ptr::eq(alt_if.as_ref(), if_stmt) {
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Walk the chain to see if there's at least one `else if` and
        // whether it ends with a bare `else`.
        let mut has_else_if = false;
        let mut current = if_stmt;
        let mut last_else_if_span = if_stmt.span();

        loop {
            match &current.alternate {
                Some(Statement::IfStatement(nested_if)) => {
                    has_else_if = true;
                    last_else_if_span = nested_if.span();
                    current = nested_if;
                }
                Some(_) => {
                    // Bare `else { ... }` — chain is complete.
                    return;
                }
                None => break,
            }
        }

        if !has_else_if {
            return;
        }

        let (line, col) = byte_offset_to_line_col(ctx.source, last_else_if_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "`if/else if` chain without a final `else` \
                      — add an `else` block to handle remaining cases."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
