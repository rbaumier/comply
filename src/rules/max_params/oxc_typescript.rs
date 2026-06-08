//! max-params OXC backend.
//!
//! Mirrors `ts-max-params` but reads its threshold from `[rules.max-params]`
//! and applies to TS/JS/TSX uniformly. Skips function expressions / arrow
//! functions passed as fixed-signature library callbacks (TanStack Query
//! `onError` / `queryFn` / etc.) since the user has no control over those
//! arities.

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
                && id.name.as_str() == "this"
            {
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
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let max_params = ctx.config.threshold("max-params", "max", ctx.lang);

        let (count, name, span) = match node.kind() {
            AstKind::Function(func) => (count_params(&func.params), func_name(func), func.span()),
            AstKind::ArrowFunctionExpression(arrow) => {
                (count_params(&arrow.params), "<anonymous>", arrow.span())
            }
            _ => return,
        };

        if count > max_params && !is_fixed_signature_library_callback(node, semantic) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Function `{name}` has {count} parameters (maximum allowed is {max_params})."
                ),
                severity: Severity::Error,
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
        // `max-params` default is 3 — 4 params triggers the rule.
        let src = "function foo(a: number, b: number, c: number, d: number) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_three_params() {
        let src = "function foo(a: number, b: string, c: boolean) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_mutation_on_error_callback_with_four_params() {
        // Regression for rbaumier/comply#203 — TanStack Query callback
        // signatures are dictated by the library types. 4 params exceeds
        // the default max=3 threshold, so the exemption must fire.
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
    fn allows_use_query_query_fn_with_five_params() {
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            useQuery({
                queryFn: (a, b, c, d, e) => 1,
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mutation_cache_on_error_callback_issue_587() {
        // Regression for rbaumier/comply#587 — `new MutationCache({ onError })`
        // receives React Query's fixed 4-parameter callback signature, dictated
        // by the library types, not by the caller.
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
    fn still_flags_unknown_factory_callback_with_four_params() {
        // `myFn` is not in the allowlist — the callback is still flagged.
        let src = r#"
            myFn({
                onError: (a, b, c, d) => 1,
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_infinite_query_get_next_page_param() {
        // Regression: getNextPageParam / getPreviousPageParam are v5 infinite-query
        // callbacks with a 4-arg signature dictated by TanStack Query.
        let src = r#"
            import { useInfiniteQuery } from "@tanstack/react-query";
            useInfiniteQuery({
                getNextPageParam: (lastPage, allPages, lastPageParam, allPageParams) => null,
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_infinite_query_v4_three_arg_overload() {
        // Regression for rbaumier/comply#207 — TanStack Query v4 supports
        // the `useInfiniteQuery(queryKey, queryFn, options)` overload where
        // the options object (and its fixed-signature callbacks) is the
        // third argument, not the first.
        let src = r#"
            import { useInfiniteQuery } from "@tanstack/react-query";
            useInfiniteQuery(["k"], () => fetch("/x"), {
                getNextPageParam: (lastPage, allPages, lastPageParam, allPageParams) => null,
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_use_query_v4_three_arg_overload_on_error() {
        // v4 `useQuery(queryKey, queryFn, options)` overload — onError lives
        // in the third argument.
        let src = r#"
            import { useQuery } from "@tanstack/react-query";
            useQuery(["k"], () => fetch("/x"), {
                onError: (error, variables, context, mutation) => {},
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_namespace_import_use_mutation_on_error() {
        // Regression: namespace-import call `RQ.useMutation(...)` must be
        // recognised via StaticMemberExpression callee matching.
        let src = r#"
            import * as RQ from "@tanstack/react-query";
            RQ.useMutation({
                onError: (a, b, c, d) => {},
            });
        "#;
        assert!(run(src).is_empty());
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
