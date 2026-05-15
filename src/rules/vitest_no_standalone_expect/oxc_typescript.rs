//! vitest-no-standalone-expect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_BLOCKS: &[&str] = &[
    "test",
    "it",
    "describe",
    "suite",
    "beforeAll",
    "beforeEach",
    "afterAll",
    "afterEach",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

/// Walk up ancestors looking for a CallExpression whose callee is one
/// of the known test blocks. Returns true if found.
fn inside_test_block<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            let name = match &call.callee {
                Expression::Identifier(id) => id.name.as_str(),
                Expression::StaticMemberExpression(m) => {
                    if let Expression::Identifier(obj) = &m.object {
                        obj.name.as_str()
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            if TEST_BLOCKS.contains(&name) {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["expect("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "expect" {
            return;
        }
        if inside_test_block(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`expect(...)` outside any test block — it runs at import time, \
                      not as part of a test. Move it into `test(...)` or `beforeAll(...)`."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}
