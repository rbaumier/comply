//! OXC backend for tanstack-query-prefer-query-options.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::{Argument, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryOptions"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // First pass: check if the file already uses `queryOptions()`.
        let mut has_query_options = false;
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && let Expression::Identifier(id) = &call.callee
                    && id.name.as_str() == "queryOptions" {
                        has_query_options = true;
                        break;
                    }
        }

        if has_query_options {
            return Vec::new();
        }

        // Second pass: flag `useQuery({ ... })` with inline object.
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::CallExpression(call) = node.kind()
                && let Expression::Identifier(id) = &call.callee
                    && id.name.as_str() == "useQuery"
                        && let Some(first) = call.arguments.first()
                            && matches!(first, Argument::ObjectExpression(_)) {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "Extract inline `useQuery` options to a `queryOptions()` factory for reuse and type-safety.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_inline_options() {
        assert_eq!(
            run("useQuery({ queryKey: ['users'], queryFn: fetchUsers })").len(),
            1
        );
    }


    #[test]
    fn allows_query_options_factory() {
        assert!(run(
            "const opts = queryOptions({ queryKey: ['users'], queryFn: fetchUsers })\nuseQuery(opts)"
        )
        .is_empty());
    }
}
