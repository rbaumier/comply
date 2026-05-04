use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@deprecated"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }

            let prefix_start = start.saturating_sub(2);
            let with_prefix = &ctx.source[prefix_start..end];
            if !with_prefix.starts_with("/*") {
                continue;
            }

            let text = &ctx.source[start..end];
            let Some(dep_pos) = text.find("@deprecated") else {
                continue;
            };

            let after = text[dep_pos + "@deprecated".len()..].trim_start();
            if !after.is_empty() && !after.starts_with('*') && !after.starts_with('\n') {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, start + dep_pos);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`@deprecated` without a migration message — \
                          add text after the tag explaining what to use instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
