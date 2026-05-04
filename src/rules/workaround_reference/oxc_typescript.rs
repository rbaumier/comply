use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["workaround", "hack", "compat", "Workaround", "Hack", "Compat", "HACK"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }
            let text = &ctx.source[start..end];

            if !super::has_keyword(text) {
                continue;
            }
            if super::has_reference(text) {
                continue;
            }

            let (line, _) = byte_offset_to_line_col(ctx.source, start);
            let row = line.saturating_sub(1);
            let lookahead = (row + 1..=(row + 2).min(lines.len().saturating_sub(1)))
                .any(|i| super::has_reference(lines[i]));
            if lookahead {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Workaround/hack/compat comment without an issue reference — \
                          add a link or ticket number."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
