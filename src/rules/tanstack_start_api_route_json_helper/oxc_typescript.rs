//! OXC backend for tanstack-start-api-route-json-helper.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

use oxc_ast::ast::{Argument, Expression};

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@tanstack/start", "@tanstack/react-start"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        // Constructor must be `Response`.
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if ctor.name.as_str() != "Response" {
            return;
        }

        // First argument must be `JSON.stringify(...)`.
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };
        if !is_json_stringify_call(first_arg) {
            return;
        }

        // File must use TanStack Start, where `json()` is importable.
        if !ctx.source_contains("@tanstack/start")
            && !ctx.source_contains("@tanstack/react-start")
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `json(data)` from `@tanstack/react-start` instead of \
                      `new Response(JSON.stringify(data))`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_json_stringify_call(arg: &Argument) -> bool {
    let Argument::CallExpression(call) = arg else {
        return false;
    };
    let Expression::StaticMemberExpression(mem) = &call.callee else {
        return false;
    };
    if mem.property.name.as_str() != "stringify" {
        return false;
    }
    matches!(&mem.object, Expression::Identifier(id) if id.name.as_str() == "JSON")
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    const TANSTACK_IMPORT: &str = "import { json } from '@tanstack/react-start';\n";

    #[test]
    fn flags_new_response_json_stringify_in_tanstack_file() {
        let src = format!("{TANSTACK_IMPORT}return new Response(JSON.stringify(data));");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_with_headers_opts_in_tanstack_file() {
        let src = format!(
            "{TANSTACK_IMPORT}return new Response(JSON.stringify(data), {{ headers: {{ 'content-type': 'application/json' }} }});"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn ignores_when_no_tanstack_start_import() {
        let src = "const mockResponse = new Response(JSON.stringify(body), { status: 400, headers: { 'Content-Type': 'application/problem+json' } });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_new_response_text_in_tanstack_file() {
        let src = format!("{TANSTACK_IMPORT}return new Response('hello');");
        assert!(run(&src).is_empty());
    }
}
