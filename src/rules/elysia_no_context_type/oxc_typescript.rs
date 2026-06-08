//! OxcCheck backend — flag manual `Context` type annotations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::FormalParameter(param) = node.kind() else { continue };

            let Some(ref type_ann) = param.type_annotation else { continue };
            let text = &ctx.source[type_ann.type_annotation.span().start as usize..type_ann.type_annotation.span().end as usize];
            if text.trim() == "Context" {
                let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Parameter typed as `Context` — Elysia infers the context type per-route. Destructure inline instead.".into(),
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
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_context_param() {
        let src = "import { Context } from 'elysia';\nfunction h(ctx: Context) { return 1; }";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_context_arrow_param() {
        let src = "import { Elysia } from 'elysia';\nconst h = (context: Context) => 1;";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_destructured_param() {
        let src = "import { Elysia } from 'elysia';\nconst h = ({ body, set }) => 1;";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "function h(ctx: Context) { return 1; }";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
