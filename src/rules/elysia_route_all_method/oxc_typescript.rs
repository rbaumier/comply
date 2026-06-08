//! elysia-route-all-method oxc backend — flag `.all(` in Elysia route chains.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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

        // Callee must be a member expression with property "all".
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "all" {
            return;
        }

        // Require at least 1 arg that starts with a string literal (path).
        let Some(first_arg) = call.arguments.first() else { return };
        let first_text = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        if !(first_text.starts_with('\'') || first_text.starts_with('"') || first_text.starts_with('`')) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.all()` matches any HTTP method — prefer a specific method (`.get`, `.post`, etc.) to communicate intent.".into(),
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
    fn flags_all_route() {
        let src = "import { Elysia } from 'elysia';\napp.all('/users', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_chained_all() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().all('/health', handler);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_specific_method() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.all('/users', () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
