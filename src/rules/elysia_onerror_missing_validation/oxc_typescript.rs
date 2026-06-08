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

        // Skip optional calls (`options.onError?.()`) — those invoke stored
        // TanStack Query callbacks, not Elysia lifecycle hook registrations.
        if call.optional {
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &crate::project::ProjectCtx::for_test_with_framework("elysia"), crate::rules::file_ctx::default_static_file_ctx())
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

    #[test]
    fn ignores_use_mutation_optional_onerror_forwarding() {
        // Regression for #377: a useMutation wrapper that forwards the
        // caller's optional `onError` callback via `options.onError?.()`
        // must not be flagged — it's a TanStack Query invocation pattern,
        // not an Elysia hook registration.
        let src = "import { useMutation } from '@tanstack/react-query';\n\
            export function useFormMutation(options) {\n\
              return useMutation({\n\
                onError: (error, variables, context, mutation) => {\n\
                  options.onError?.(error, variables, context, mutation);\n\
                },\n\
              });\n\
            }";
        assert!(run_on(src).is_empty());
    }
}
