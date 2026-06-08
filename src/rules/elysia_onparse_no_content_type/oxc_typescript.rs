//! elysia-onparse-no-content-type oxc backend.

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
        Some(&["onParse"])
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

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "onParse" {
            return;
        }

        let args_text = &ctx.source[call.span.start as usize..call.span.end as usize];
        if args_text.contains("contentType") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`onParse` handler should inspect `contentType` and only handle formats it understands; otherwise it can break default parsing.".into(),
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
    fn flags_onparse_without_content_type() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(({ request }) => request.text());";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_onparse_with_unrelated_logic() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(async ({ request }) => {\n  const body = await request.text();\n  return JSON.parse(body);\n});";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_onparse_with_content_type_check() {
        let src = "import { Elysia } from 'elysia';\napp.onParse(({ request, contentType }) => {\n  if (contentType === 'application/x-yaml') return parseYaml(request);\n});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.onParse(() => null);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
