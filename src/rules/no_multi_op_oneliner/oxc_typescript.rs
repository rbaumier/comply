//! no-multi-op-oneliner oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let min_ops = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_ops", ctx.lang);
        let min_line_length = ctx
            .config
            .threshold("no-multi-op-oneliner", "min_line_length", ctx.lang);

        let line_offsets = super::dense_lines::compute_line_offsets(ctx.source);

        // Collect comment byte ranges from OXC semantic.
        let comment_ranges: Vec<(usize, usize)> = semantic
            .comments()
            .iter()
            .map(|c| (c.span.start as usize, c.span.end as usize))
            .collect();

        let mut reported_lines = std::collections::HashSet::new();
        let mut diagnostics = Vec::new();

        for node in semantic.nodes() {
            let is_target = matches!(
                node.kind(),
                AstKind::ExpressionStatement(_) | AstKind::VariableDeclarator(_)
            );
            if !is_target {
                continue;
            }
            let span = match node.kind() {
                AstKind::ExpressionStatement(s) => s.span,
                AstKind::VariableDeclarator(d) => d.span,
                _ => continue,
            };
            let (start_line, _) = byte_offset_to_line_col(ctx.source, span.start as usize);
            let (end_line, _) = byte_offset_to_line_col(ctx.source, span.end as usize);
            if start_line != end_line {
                continue;
            }
            if reported_lines.contains(&start_line) {
                continue;
            }
            // line_offsets is 0-indexed, start_line is 1-indexed.
            let Some(&(line_start_byte, line_text)) = line_offsets.get(start_line - 1) else {
                continue;
            };
            let stripped =
                super::dense_lines::strip_comments(line_text, line_start_byte, &comment_ranges);
            if stripped.len() < min_line_length {
                continue;
            }
            let ops = super::dense_lines::count_operators(&stripped);
            if ops < min_ops {
                continue;
            }
            reported_lines.insert(start_line);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: start_line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Line has {ops} chained operations — extract intermediate \
                     named bindings so each step's purpose is visible."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
