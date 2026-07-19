//! elysia-prefer-status-over-set oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_inside_onerror_callback};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        let left_span = assign.left.span();
        let left_text = &ctx.source[left_span.start as usize..left_span.end as usize];

        // Match `set.status` on the left side.
        if left_text != "set.status" {
            return;
        }

        // Inside an `.onError(...)` handler, mutating `set.status` while
        // returning the body is the idiomatic shape — not a violation (#534).
        if is_inside_onerror_callback(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`set.status = code` is untyped \u{2014} use `status(code, body)` for type-safe responses.".into(),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            "t.ts",
            &crate::project::ProjectCtx::for_test_with_framework("elysia"),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn flags_set_status_in_route_handler() {
        let src = "import { Elysia } from 'elysia';\n\
            new Elysia().get('/', ({ set }) => { set.status = 404; return 'nope'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_set_status_at_module_scope() {
        let src = "import { Elysia } from 'elysia';\nset.status = 500;";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for #534: a scope-qualified `.onError({ as: 'global' }, ...)`
    // handler mutates `set.status` separately while returning the body — the
    // idiomatic Elysia shape, not a violation.
    #[test]
    fn allows_set_status_in_global_onerror_issue_534() {
        let src = "import { Elysia } from 'elysia';\n\
            new Elysia().onError({ as: 'global' }, ({ error, code, set }) => {\n\
              set.status = apiError.status;\n\
              return errorToProblem(apiError, requestId);\n\
            });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_set_status_in_bare_onerror() {
        let src = "import { Elysia } from 'elysia';\n\
            new Elysia().onError(({ set }) => { set.status = 500; return 'oops'; });";
        assert!(run_on(src).is_empty());
    }

    // A route handler in the same chain as an `.onError` is still flagged: the
    // exemption is lexical, scoped to the error callback only.
    #[test]
    fn flags_route_handler_alongside_onerror() {
        let src = "import { Elysia } from 'elysia';\n\
            new Elysia()\n\
              .onError(({ set }) => { set.status = 500; return 'err'; })\n\
              .get('/', ({ set }) => { set.status = 201; return 'ok'; });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "set.status = 500;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.ts").is_empty());
    }
}
