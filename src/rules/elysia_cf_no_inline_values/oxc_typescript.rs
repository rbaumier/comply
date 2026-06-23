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

        // An actual Elysia route registration has a string *path* as its first
        // argument (`app.get('/users', …)`), which always starts with `/`. This
        // excludes non-route calls that merely share a method name and a string
        // second argument — notably `Reflect.get(value, "status")`, whose first
        // argument is the target object, not a route path (#5754).
        let Argument::StringLiteral(path) = &call.arguments[0] else {
            return;
        };
        if !path.value.as_str().starts_with('/') {
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
    fn flags_genuine_inline_string_route() {
        // The rule's real target: a route whose handler is an inline string.
        let src = r#"app.get("/", "Hello")"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_reflect_get_with_string_key() {
        // #5754 firing site (error-handler.ts): `Reflect.get` shares the
        // `.get(x, "string")` shape but is not a route — its first argument is
        // the target object, not a `/` route path.
        let src = r#"const status = Reflect.get(value, "status");"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn ignores_non_path_string_first_arg() {
        // Any `.get(key, default)` whose first argument is not a route path is
        // not a route registration (e.g. a cache/map-like lookup).
        let src = r#"const v = cache.get("session", "anonymous");"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn ignores_function_handler_route() {
        // A proper arrow-function handler is the remediation, not a violation.
        let src = r#"app.get("/", () => "Hello")"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
