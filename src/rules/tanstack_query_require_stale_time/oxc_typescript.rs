//! OxcCheck backend for tanstack-query-require-stale-time — flag `new QueryClient(...)` without `staleTime`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["QueryClient"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Test files create throwaway `QueryClient`s to assert cache behaviour
        // (set data, read it back); no real refetch window applies.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let AstKind::NewExpression(new_expr) = node.kind() else { return };
        let Expression::Identifier(id) = &new_expr.callee else { return };
        if id.name.as_str() != "QueryClient" {
            return;
        }
        // Check if any argument contains "staleTime" in its source text.
        for arg in &new_expr.arguments {
            let arg_text = &ctx.source[arg.span().start as usize..arg.span().end as usize];
            if arg_text.contains("staleTime") {
                return;
            }
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`QueryClient` without a default `staleTime` refetches on every component mount.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::test_helpers::{run_oxc_tsx_with_file_ctx, run_oxc_ts};

    #[test]
    fn flags_query_client_without_stale_time() {
        assert_eq!(run_oxc_ts("const c = new QueryClient();", &Check).len(), 1);
    }

    #[test]
    fn allows_query_client_with_stale_time() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { staleTime: 60_000 } } });";
        assert!(run_oxc_ts(src, &Check).is_empty());
    }

    #[test]
    fn allows_query_client_in_test_file() {
        // Regression for issue #488: throwaway QueryClient in a test file.
        let src = r#"
            const queryClient = new QueryClient();
            queryClient.setQueryData(usersQueryOptions(query).queryKey, prefetched);
        "#;
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        assert!(run_oxc_tsx_with_file_ctx(src, &Check, &file).is_empty());
    }
}
