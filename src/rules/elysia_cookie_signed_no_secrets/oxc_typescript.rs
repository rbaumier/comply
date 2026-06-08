//! elysia-cookie-signed-no-secrets oxc backend — flag t.Cookie(..., { sign: ... }) without secrets in file.

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

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "t.Cookie" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
        if !norm.contains("sign:") {
            return;
        }

        if ctx.source_contains("secrets:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Cookie uses `sign:` but no `secrets:` is configured — signature cannot be verified.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_signed_without_secrets() {
        let src = "import { Elysia, t } from 'elysia';\nconst c = t.Cookie({ token: t.String() }, { sign: ['token'] });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_signed_with_secrets() {
        let src = "import { Elysia, t } from 'elysia';\nnew Elysia({ cookie: { secrets: 'k' } });\nconst c = t.Cookie({ token: t.String() }, { sign: ['token'] });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const c = t.Cookie({ token: t.String() }, { sign: ['token'] });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
