//! elysia-cf-no-inline-values — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "head", "all",
];

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

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Extract the method name from the callee (e.g. `app.get`).
        let method = match &call.callee {
            Expression::StaticMemberExpression(member) => member.property.name.as_str(),
            _ => return,
        };
        if !ROUTE_METHODS.contains(&method) {
            return;
        }

        // Need at least two arguments: path + handler.
        if call.arguments.len() < 2 {
            return;
        }

        // Check if the second argument is a string literal.
        let second = &call.arguments[1];
        if !matches!(second, Argument::StringLiteral(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Inline string handler under `CloudflareAdapter` — wrap the value in an arrow function.".into(),
            severity: Severity::Error,
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
    fn flags_inline_string_handler() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/', 'Hello');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_function_handler() {
        let src = "import { CloudflareAdapter } from 'elysia/adapter/cloudflare';\napp.get('/', () => 'Hello');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_cf_files() {
        let src = "app.get('/', 'Hello');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
