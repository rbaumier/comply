//! testing-no-conditional-assertion OXC backend.
//!
//! Flag `expect(...)` calls inside an `if`-statement body within a
//! `test()` / `it()` callback.

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
        // Must be a bare `expect(...)` call.
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "expect" {
            return;
        }

        // Walk ancestors: need both an if-statement body and a test/it call.
        let mut in_if_body = false;
        let mut in_test = false;
        let nodes = semantic.nodes();
        let mut cur_id = nodes.parent_id(node.id());
        loop {
            if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
                break;
            }
            let parent_kind = nodes.kind(cur_id);
            match parent_kind {
                AstKind::IfStatement(if_stmt) => {
                    use oxc_span::GetSpan;
                    let test_span = if_stmt.test.span();
                    let call_span = call.span;
                    if call_span.start < test_span.start || call_span.start >= test_span.end {
                        in_if_body = true;
                    }
                }
                AstKind::CallExpression(ancestor_call) => {
                    if let Expression::Identifier(id) = &ancestor_call.callee {
                        let n = id.name.as_str();
                        if n == "test" || n == "it" {
                            in_test = true;
                        }
                    }
                }
                _ => {}
            }
            if in_if_body && in_test {
                break;
            }
            cur_id = nodes.parent_id(cur_id);
        }

        if in_if_body && in_test {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "expect(...) inside an if-branch silently skips when the branch is not taken \u{2014} make the assertion unconditional.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
