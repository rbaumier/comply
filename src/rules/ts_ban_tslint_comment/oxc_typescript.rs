//! OxcCheck backend for ts-ban-tslint-comment — flag `tslint:enable` / `tslint:disable`
//! comment directives. Comments are not AST nodes in OXC so we iterate
//! `semantic.comments()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["tslint"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments().iter() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };

            // Strip leading // or /* and whitespace to get the content.
            let stripped = raw
                .trim_start_matches('/')
                .trim_start_matches('*')
                .trim();

            if stripped.starts_with("tslint:enable") || stripped.starts_with("tslint:disable") {
                let text = raw.trim();
                let (line, column) = byte_offset_to_line_col(ctx.source, start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("TSLint comment detected: `{text}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
    fn flags_tslint_disable() {
        let diags = run_on("// tslint:disable\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("tslint"));
    }


    #[test]
    fn flags_tslint_enable() {
        let diags = run_on("// tslint:enable\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_tslint_disable_next_line() {
        let diags = run_on("// tslint:disable-next-line: no-any\nconst x: any = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_normal_comments() {
        let diags = run_on("// This uses tslint-style formatting\nconst x = 1;");
        assert!(diags.is_empty());
    }
}
