//! playwright-require-top-level-describe oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_playwright_file(source: &str) -> bool {
    source.contains("@playwright/test") || source.contains("playwright/test")
}

fn is_bare_test_call(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    id.name.as_str() == "test"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["test("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_playwright_file(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_bare_test_call(call) {
            return;
        }
        // Direct parent must be ExpressionStatement, grandparent
        // must be Program (i.e. module top).
        let parent = semantic.nodes().parent_node(node.id());
        let AstKind::ExpressionStatement(_) = parent.kind() else {
            return;
        };
        let grand = semantic.nodes().parent_node(parent.id());
        if !matches!(grand.kind(), AstKind::Program(_)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Top-level `test(...)` — wrap in `test.describe(\"<feature>\", \
                      () => { ... })` so reports group related cases."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
