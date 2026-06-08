//! elysia-derive-async-no-await oxc backend — flag `.derive(async ...)` whose body
//! contains no `await`.

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

        let AstKind::CallExpression(call) = node.kind() else { return };

        // Callee must be a member expression with property "derive".
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "derive" {
            return;
        }

        // Check argument text for async without await.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let text = &ctx.source[args_start..args_end];
        if !text.contains("async") {
            return;
        }
        if text.contains("await ") || text.contains("await(") {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.derive(async ...)` body never awaits — handlers receive a Promise and must explicitly await it. Drop `async` or add an `await`.".into(),
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
    fn flags_async_derive_no_await() {
        let src = "import { Elysia } from 'elysia';\napp.derive(async () => ({ id: 1 }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_async_derive_with_await() {
        let src = "import { Elysia } from 'elysia';\napp.derive(async () => ({ user: await getUser() }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_sync_derive() {
        let src = "import { Elysia } from 'elysia';\napp.derive(() => ({ id: 1 }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.derive(async () => ({ id: 1 }));";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
