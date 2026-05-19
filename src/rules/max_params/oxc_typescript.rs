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
mod oxc_tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
