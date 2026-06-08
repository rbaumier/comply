//! ts-ban-ts-comment OXC backend — flag @ts-ignore, @ts-nocheck, and bare
//! @ts-expect-error via semantic comments.

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
        Some(&["@ts-"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            // OXC comment spans INCLUDE the `//` or `/* */` markers
            let text = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            let stripped = text.trim_start_matches('/').trim_start_matches('*').trim();

            if let Some(_rest) = stripped.strip_prefix("@ts-ignore") {
                let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `@ts-expect-error` instead of `@ts-ignore`, as `@ts-ignore` will do nothing if the following line is error-free.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if let Some(_rest) = stripped.strip_prefix("@ts-nocheck") {
                let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Do not use `@ts-nocheck` because it alters compilation errors.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if let Some(rest) = stripped.strip_prefix("@ts-expect-error") {
                let description = rest.trim();
                if description.is_empty() || description.len() < 3 {
                    let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Include a description after `@ts-expect-error` to explain why it is necessary (at least 3 characters).".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
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
    fn flags_ts_ignore() {
        let diags = run_on("// @ts-ignore\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@ts-expect-error"));
    }


    #[test]
    fn flags_ts_nocheck() {
        let diags = run_on("// @ts-nocheck\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@ts-nocheck"));
    }


    #[test]
    fn flags_bare_ts_expect_error() {
        let diags = run_on("// @ts-expect-error\nconst x = 1;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("description"));
    }


    #[test]
    fn allows_ts_expect_error_with_description() {
        let diags = run_on("// @ts-expect-error legacy API returns wrong type\nconst x = 1;");
        assert!(diags.is_empty());
    }
}
