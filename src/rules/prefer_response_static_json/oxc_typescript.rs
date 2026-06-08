//! prefer-response-static-json oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Response"])
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
        let Expression::Identifier(ident) = &new_expr.callee else {
            return;
        };
        if ident.name.as_str() != "Response" {
            return;
        }

        // Must have at least one argument.
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };

        // First argument must be a call expression: JSON.stringify(...).
        let oxc_ast::ast::Argument::CallExpression(call) = first_arg else {
            return;
        };

        // Callee must be `JSON.stringify`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" || member.property.name.as_str() != "stringify" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `Response.json(data)` over `new Response(JSON.stringify(data))`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_new_response_json_stringify() {
        let d = run_on(
            r#"return new Response(JSON.stringify(data), { headers: { "Content-Type": "application/json" } });"#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-response-static-json");
    }


    #[test]
    fn flags_bare_new_response_json_stringify() {
        let d = run_on("const res = new Response(JSON.stringify({ ok: true }));");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_response_json() {
        assert!(run_on("return Response.json(data);").is_empty());
    }


    #[test]
    fn allows_new_response_with_string() {
        assert!(run_on(r#"return new Response("hello");"#).is_empty());
    }
}
