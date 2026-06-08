//! OxcCheck backend — flag `.macro({ ... })` bulk form.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Argument;
use oxc_span::GetSpan;
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

        let AstKind::CallExpression(call) = node.kind() else { return };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if !callee_text.ends_with(".macro") {
            return;
        }

        // Check if the first argument is an object expression (bulk form).
        let Some(first_arg) = call.arguments.first() else { return };
        let is_object = matches!(first_arg, Argument::ObjectExpression(_));
        if !is_object {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.macro({ ... })` bulk form blocks cross-macro inference — use `.macro('name', { ... })`.".into(),
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
    fn flags_object_first_arg() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().macro({ isAuth: { resolve() {} } });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_named_form() {
        let src =
            "import { Elysia } from 'elysia';\nnew Elysia().macro('isAuth', { resolve() {} });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "obj.macro({ isAuth: {} });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
