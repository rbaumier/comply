//! playwright-expect-expect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];

fn is_test_callee(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(id) => TEST_FNS.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                TEST_FNS.contains(&obj.name.as_str())
            } else {
                false
            }
        }
        _ => false,
    }
}

fn callback_contains_expect(source: &str, start: usize, end: usize) -> bool {
    let slice = &source[start..end];
    let bytes = slice.as_bytes();
    let mut search_from = 0;
    while search_from + 7 <= bytes.len() {
        if let Some(pos) = slice[search_from..].find("expect(") {
            let abs = search_from + pos;
            let before_ok =
                abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
            if before_ok {
                return true;
            }
            search_from = abs + 7;
        } else {
            break;
        }
    }
    false
}

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }
        if !is_test_callee(&call.callee) {
            return;
        }

        // The callback is typically the last argument.
        if call.arguments.is_empty() {
            return;
        }
        let last_arg = &call.arguments[call.arguments.len() - 1];
        let callback_span = match last_arg {
            Argument::ArrowFunctionExpression(arrow) => arrow.span,
            Argument::FunctionExpression(func) => func.span(),
            _ => return,
        };

        if !callback_contains_expect(
            ctx.source,
            callback_span.start as usize,
            callback_span.end as usize,
        ) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Test has no assertions.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
