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
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    

    #[test]
    fn flags_query_client_without_stale_time() {
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, "const c = new QueryClient();", "t.ts").len(), 1);
    }

    #[test]
    fn allows_query_client_with_stale_time() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { staleTime: 60_000 } } });";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
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
        assert!(crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file).is_empty());
    }
}
