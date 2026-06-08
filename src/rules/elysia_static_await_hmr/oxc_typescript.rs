//! OXC backend for elysia-static-await-hmr.

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
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };
        if !ctx.project.has_framework("elysia") {
            return;
        }

        // callee must be `staticPlugin`
        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "staticPlugin" {
            return;
        }

        // If parent is an AwaitExpression, it's fine.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::AwaitExpression(_)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`staticPlugin()` is async — use `await staticPlugin()` so HMR picks up file changes.".into(),
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
    fn flags_use_static_plugin_without_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nnew Elysia().use(staticPlugin());";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_use_static_plugin_with_await() {
        let src = "import { Elysia } from 'elysia';\nimport { staticPlugin } from '@elysiajs/static';\nasync function main() { return new Elysia().use(await staticPlugin()); }";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_files_without_static_import() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().use(staticPlugin());";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
