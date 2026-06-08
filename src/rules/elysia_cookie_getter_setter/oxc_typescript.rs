//! OxcCheck backend for elysia-cookie-getter-setter.

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
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // callee must be `cookie.get` or `cookie.set`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();
        if prop != "get" && prop != "set" {
            return;
        }
        let Expression::Identifier(obj_id) = &member.object else { return };
        if obj_id.name.as_str() != "cookie" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `cookie.<name>.value` instead of `cookie.get/set(...)` — Elysia cookies are reactive accessors.".into(),
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
    fn flags_cookie_get() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.get('session'));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_cookie_set() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.set('session', 'x'));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_cookie_value_access() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().get('/', ({ cookie }) => cookie.session.value);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "cookie.get('session');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
