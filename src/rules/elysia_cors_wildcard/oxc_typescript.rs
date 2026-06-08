//! elysia-cors-wildcard — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text =
            &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "cors" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);

        // `cors()` with no arguments allows all origins.
        if call.arguments.is_empty() {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`cors()` without arguments allows any origin to access the API.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }

        // Check if argument contains `origin: '*'`.
        let Some(first_arg) = call.arguments.first() else { return };
        let arg_text =
            &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        let norm: String = arg_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("origin:'*'") || norm.contains("origin:\"*\"") {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`origin: '*'` allows any origin to access the API.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_bare_cors() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors());";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_wildcard_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: '*' }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_specific_origin() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: 'https://example.com' }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.use(cors());";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
