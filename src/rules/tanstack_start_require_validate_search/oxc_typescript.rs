//! tanstack-start-require-validate-search OXC backend — flag
//! `Route.useSearch()` in files that lack a `validateSearch:` option.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // Skip lazy-route files (*.lazy.{ts,tsx,js,jsx}) — validateSearch is not expected here.
        let path_str = ctx.path.to_string_lossy();
        if path_str.ends_with(".lazy.tsx")
            || path_str.ends_with(".lazy.ts")
            || path_str.ends_with(".lazy.jsx")
            || path_str.ends_with(".lazy.js")
        {
            return Vec::new();
        }

        // Bail early if source already contains `validateSearch`
        if ctx.source_contains("validateSearch") {
            return Vec::new();
        }

        // Find the first `Route.useSearch()` call
        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            let Expression::StaticMemberExpression(member) = &call.callee else {
                continue;
            };
            let Expression::Identifier(obj) = &member.object else {
                continue;
            };
            if obj.name.as_str() != "Route" {
                continue;
            }
            if member.property.name.as_str() != "useSearch" {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            return vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`Route.useSearch()` without `validateSearch:` in the route config accepts untyped search params.".into(),
                severity: Severity::Warning,
                span: None,
            }];
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::{run_oxc_ts, run_oxc_ts_with_path};

    #[test]
    fn flags_use_search_without_validate() {
        assert_eq!(run_oxc_ts("const { page } = Route.useSearch()", &Check).len(), 1);
    }

    #[test]
    fn allows_with_validate_search() {
        assert!(run_oxc_ts(
            "const { page } = Route.useSearch()\nconst route = createFileRoute('/posts')({ validateSearch: z.object({ page: z.number() }) })",
            &Check,
        )
        .is_empty());
    }

    #[test]
    fn skips_lazy_route_files() {
        // Lazy-route files must not be flagged.
        assert!(
            run_oxc_ts_with_path(
                "export const Route = createLazyFileRoute('/login')({ component: () => null })\nconst { redirect } = Route.useSearch()",
                &Check,
                "src/routes/login.lazy.tsx",
            )
            .is_empty()
        );
    }

    #[test]
    fn still_flags_non_lazy_route_files() {
        // Positive control — the paired eager `login.tsx` must still be flagged.
        assert_eq!(
            run_oxc_ts_with_path(
                "export const Route = createFileRoute('/login')({ component: () => null })\nconst { redirect } = Route.useSearch()",
                &Check,
                "src/routes/login.tsx",
            )
            .len(),
            1,
        );
    }



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn ignores_no_use_search() {
        assert!(run("const route = createFileRoute('/posts')({})").is_empty());
    }
}
