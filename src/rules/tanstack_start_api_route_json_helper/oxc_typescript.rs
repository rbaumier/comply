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
        Some(&["JSON.stringify"])
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
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_new_response_json_stringify() {
        assert_eq!(
            run("return new Response(JSON.stringify({ ok: true }));").len(),
            1
        );
    }


    #[test]
    fn flags_with_headers_opts() {
        let src = "return new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_json_helper() {
        assert!(run("return json({ ok: true });").is_empty());
    }


    #[test]
    fn allows_new_response_text() {
        assert!(run("return new Response('hello');").is_empty());
    }
}
