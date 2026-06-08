//! ts-max-params OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_fixed_signature_library_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn count_params(params: &oxc_ast::ast::FormalParameters) -> usize {
    params
        .items
        .iter()
        .filter(|p| {
            // Skip TS `this` parameter
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &p.pattern
                && id.name.as_str() == "this" {
                    return false;
                }
            true
        })
        .count()
}

fn func_name<'a>(func: &'a oxc_ast::ast::Function<'a>) -> &'a str {
    func.id.as_ref().map_or("<anonymous>", |id| id.name.as_str())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let max_params = ctx.config.threshold("ts-max-params", "max", ctx.lang);

        let (count, name, span) = match node.kind() {
            AstKind::Function(func) => {
                (count_params(&func.params), func_name(func), func.span())
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                (count_params(&arrow.params), "<anonymous>", arrow.span())
            }
            _ => return,
        };

        if count > max_params && !is_fixed_signature_library_callback(node, semantic) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Function `{name}` has {count} parameters (maximum allowed is {max_params})."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_top_level_function_with_four_params() {
        let src = "function foo(a: number, b: number, c: number, d: number) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_mutation_on_error_callback_with_four_params() {
        // Regression for rbaumier/comply#203 — TanStack Query callback
        // signatures are dictated by the library types.
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            useMutation({
                onError: (error, variables, context, mutation) => {
                    console.log(error);
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_query_query_fn_with_many_params() {
        // `queryFn` receives a single context object in real TanStack, but
        // users sometimes destructure positional helpers in test
        // doubles — exempt them too since the signature is library-driven.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            useQuery({
                queryFn: (a, b, c, d) => 1,
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_cache_on_error_callback_issue_587() {
        // Regression for rbaumier/comply#587 — `new MutationCache({ onError })`
        // receives React Query's fixed 4-parameter callback signature.
        let src = r#"
            import { MutationCache } from "@tanstack/react-query";
            new MutationCache({
                onError: (error, _variables, _context, mutation) => {
                    console.log(error);
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_unknown_factory_with_callback_at_arity_violation() {
        // `myFn` is not in the TanStack factory allowlist — still flagged.
        let src = r#"
            myFn({
                onError: (a, b, c, d) => 1,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_mutate_on_error_callback() {
        // Regression for rbaumier/comply#378 — mutation.mutate() accepts the
        // same fixed-signature callbacks as useMutation() options.
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            const mutation = useMutation({ mutationFn: async (d) => d });
            mutation.mutate(data, {
                onError: (error, variables, context, extra) => {
                    handleError(error);
                },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutate_async_on_settled_callback() {
        // Regression for rbaumier/comply#378 — mutateAsync follows the same
        // pattern; onSettled has 4 params in the TanStack Query API.
        let src = r#"
            import { useMutation } from "@tanstack/react-query";
            const mutation = useMutation({ mutationFn: async (d) => d });
            mutation.mutateAsync(data, {
                onSettled: (data, error, variables, context) => {},
            });
        "#;
        assert!(run(src).is_empty());
    }
}
