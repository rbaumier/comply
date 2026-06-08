//! elysia-transform-no-schema oxc backend.

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

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_text = match &call.callee {
            Expression::StaticMemberExpression(member) => member.property.name.as_str(),
            Expression::Identifier(ident) => ident.name.as_str(),
            _ => "",
        };

        if callee_text != "transform" && callee_text != "onTransform" {
            return;
        }

        // Check if any argument text contains "body".
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let call_text = &ctx.source[args_start..args_end];
        if !call_text.contains("body") {
            return;
        }

        // File contains a body schema declaration somewhere.
        if ctx.source_contains("body: t.") || ctx.source_contains("body:t.") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`transform` accesses `body` but no `body:` schema is declared \u{2014} declare one so the body is validated before mutation.".into(),
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
    fn flags_transform_without_schema() {
        let src = "import { Elysia } from 'elysia';\napp.transform(({ body }) => { body.email = body.email.toLowerCase(); });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_on_transform_without_schema() {
        let src = "import { Elysia } from 'elysia';\napp.onTransform(({ body }) => { body.normalized = true; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_transform_with_body_schema() {
        let src = "import { Elysia, t } from 'elysia';\napp.post('/u', handler, { body: t.Object({ email: t.String() }) });\napp.transform(({ body }) => { body.email = body.email.toLowerCase(); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.transform(({ body }) => body);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
