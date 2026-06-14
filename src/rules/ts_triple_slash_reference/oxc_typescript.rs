use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["///"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for line_text in ctx.source.lines() {
            let trimmed = line_text.trim();
            if !trimmed.starts_with("/// <reference") && !trimmed.starts_with("///<reference") {
                continue;
            }
            // Only `path=` references import a file and have a clean ES `import`
            // replacement. `types=` (ambient `@types` / global augmentations) and
            // `lib=` (built-in libs) pull in declarations with no ESM equivalent.
            if !trimmed.contains("path=") {
                continue;
            }
            let byte_offset = line_text.as_ptr() as usize - ctx.source.as_ptr() as usize;
            let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Triple-slash `path` reference directive is legacy — \
                          use ES `import` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}
