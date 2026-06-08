//! elysia-eden-null-body OXC backend — flag `undefined` body argument in Eden mutations.

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

        // Callee must be a member expression with property post/put/patch.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let prop_text = member.property.name.as_str();
        if !matches!(prop_text, "post" | "put" | "patch") {
            return;
        }

        // First argument must be `undefined`.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let arg_expr = first_arg.to_expression();
        let Expression::Identifier(ident) = arg_expr else {
            return;
        };
        if ident.name.as_str() != "undefined" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Eden mutation called with `undefined` body — pass `null` instead so the request serializes correctly.".into(),
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
    fn flags_undefined_post_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post(undefined, { headers: {} });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_undefined_patch_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait api.users({ id: 1 }).patch(undefined);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_null_body() {
        let src = "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post(null);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_payload_body() {
        let src =
            "import { treaty } from '@elysiajs/eden';\nawait treaty.users.post({ name: 'a' });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_eden_files() {
        let src = "fetch.post(undefined);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
