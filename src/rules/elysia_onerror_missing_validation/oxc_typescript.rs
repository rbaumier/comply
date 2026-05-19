//! elysia-onerror-missing-validation oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onError"])
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

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "onError" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if args_text.contains("VALIDATION") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`onError` handler doesn't branch on `'VALIDATION'` \u{2014} schema errors will surface as generic 500s.".into(),
            severity: Severity::Warning,
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
    fn flags_elysia_onerror_without_validation() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(({ error }) => 'oops');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_elysia_onerror_with_validation_branch() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().onError(({ code, error }) => code === 'VALIDATION' ? error.message : 'oops');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_use_mutation_on_error_object_property() {
        // Regression for #202: `useMutation({ onError: ... })` is a TanStack
        // Query callback, not an Elysia lifecycle hook. The rule must only
        // fire on `.onError(...)` member-call form.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            useMutation({ onError: (error, variables, context, mutation) => { console.log(error); } });";
        assert!(run_on(src).is_empty());
    }
}
