//! elysia-resolve-outside-guard oxc backend — flag top-level `.resolve(`.

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
        Some(&[".resolve"])
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
        if ctx.source.contains(".guard(") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "resolve" {
            return;
        }

        // Skip the global `Promise.resolve(...)` — it is not an Elysia chain.
        if let Expression::Identifier(ident) = &member.object {
            if ident.name.as_str() == "Promise" {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.resolve()` is used outside `.guard()` — derived values leak to every route in the chain.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_framework;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_top_level_resolve_on_new_elysia() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_top_level_resolve_on_app() {
        let src = "import { Elysia } from 'elysia';\napp.resolve(({ headers }) => ({ user: headers.x }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_resolve_inside_guard() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().guard({}, app => app.resolve(({ headers }) => ({ user: headers.x })));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_promise_resolve_static() {
        let src = "async function makeAsync(): Promise<number> {\n  await Promise.resolve();\n  return 42;\n}";
        assert!(run_on(src).is_empty(), "Promise.resolve() must not be flagged");
    }

    #[test]
    fn ignores_promise_resolve_with_arg() {
        let src = "await Promise.resolve(42);";
        assert!(run_on(src).is_empty(), "Promise.resolve(42) must not be flagged");
    }
}
