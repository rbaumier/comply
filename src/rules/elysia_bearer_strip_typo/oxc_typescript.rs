//! OxcCheck backend for elysia-bearer-strip-typo — flag .replace('Bearer', ...) without trailing space.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
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
        // Check callee is a `.replace` member expression.
        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !callee_text.ends_with(".replace") {
            return;
        }
        // Check the arguments text for 'Bearer' or "Bearer" (without trailing space).
        if call.arguments.is_empty() {
            return;
        }
        use oxc_span::GetSpan;
        let first_span = call.arguments.first().unwrap().span();
        let last_span = call.arguments.last().unwrap().span();
        let args_text = &ctx.source[first_span.start as usize..last_span.end as usize];
        let bad = args_text.contains("'Bearer'") || args_text.contains("\"Bearer\"");
        if !bad {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.replace('Bearer', '')` leaves a leading space in the token — use `'Bearer '` with trailing space.".into(),
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
    fn flags_missing_space() {
        let src = "import { Elysia } from 'elysia';\nconst t = h.replace('Bearer', '');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_with_trailing_space() {
        let src = "import { Elysia } from 'elysia';\nconst t = h.replace('Bearer ', '');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const t = h.replace('Bearer', '');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
