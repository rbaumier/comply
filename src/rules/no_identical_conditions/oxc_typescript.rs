//! no-identical-conditions oxc backend — flag duplicate conditions in
//! if/else-if chains.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else { return };

        // Only process the top-level if (not nested else-if branches).
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::IfStatement(_) = parent.kind() {
            // This if is the alternate of another if — it's an else-if branch.
            // We skip it to avoid double-processing the chain.
            // But we need to be more precise: only skip if this node is the
            // alternate (not the consequent) of the parent if.
            // In oxc, else-if is parsed as IfStatement directly in the alternate
            // field, so any IfStatement whose parent is also an IfStatement
            // *and* this node is that parent's alternate is an else-if branch.
            return;
        }

        // Collect all conditions in this if/else-if chain.
        let mut conditions: Vec<String> = Vec::new();
        let mut current = Some(stmt);

        while let Some(if_node) = current {
            let cond_start = if_node.test.span().start as usize;
            let cond_end = if_node.test.span().end as usize;
            let cond_text = &ctx.source[cond_start..cond_end];

            // Check for duplicates.
            for prev_text in &conditions {
                if prev_text == cond_text {
                    let (line, column) = byte_offset_to_line_col(ctx.source, cond_start);
                    let trimmed = cond_text
                        .trim_start_matches('(')
                        .trim_end_matches(')');
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Duplicate condition `{}` in if/else-if chain.",
                            trimmed
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                    break;
                }
            }
            conditions.push(cond_text.to_string());

            // Follow the else clause to the next if_statement.
            current = None;
            if let Some(Statement::IfStatement(next_if)) = &if_node.alternate {
                current = Some(next_if);
            }
        }
    }
}
