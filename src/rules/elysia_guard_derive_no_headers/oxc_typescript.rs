//! elysia-guard-derive-no-headers — oxc backend.

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
        Some(&["headers.auth", "headers.authorization"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // Callee must be `.guard`.
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "guard" {
            return;
        }

        // Get entire args text.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let args_text = ctx.source.get(args_start..args_end).unwrap_or("");

        // Need a header read.
        let reads_header =
            args_text.contains("headers.authorization") || args_text.contains("headers.auth");
        if !reads_header {
            return;
        }

        // First arg should be an object expression.
        let Some(first_arg) = call.arguments.first() else { return };
        let oxc_ast::ast::Argument::ObjectExpression(obj) = first_arg else { return };

        let config_start = obj.span.start as usize;
        let config_end = obj.span.end as usize;
        let config_text = ctx.source.get(config_start..config_end).unwrap_or("");
        let norm: String = config_text.chars().filter(|c| !c.is_whitespace()).collect();
        if norm.contains("headers:") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Guard reads `headers.authorization` without a `headers:` schema \u{2014} add one so the field is validated.".into(),
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
    fn flags_guard_with_header_read_and_no_schema() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_resolve_reading_header() {
        let src = "import { Elysia } from 'elysia';\napp.guard({ resolve: ({ headers }) => ({ token: headers.authorization }) }, app => app);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_guard_with_headers_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.guard({ headers: t.Object({ authorization: t.String() }), beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src =
            "app.guard({ beforeHandle: ({ headers }) => headers.authorization }, app => app);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
