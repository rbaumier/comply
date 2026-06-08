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
            if !trimmed.contains("path=") && !trimmed.contains("types=") {
                continue;
            }
            let byte_offset = line_text.as_ptr() as usize - ctx.source.as_ptr() as usize;
            let (line, column) = byte_offset_to_line_col(ctx.source, byte_offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Triple-slash reference directive is legacy — \
                          use ES `import` instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_path_reference() {
        let diags = run_on("/// <reference path=\"foo\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_types_reference() {
        let diags = run_on("/// <reference types=\"node\" />\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_lib_reference() {
        assert!(run_on("/// <reference lib=\"es2015\" />\nconst x = 1;").is_empty());
    }


    #[test]
    fn allows_regular_comments() {
        assert!(run_on("// just a comment\nconst x = 1;").is_empty());
    }
}
