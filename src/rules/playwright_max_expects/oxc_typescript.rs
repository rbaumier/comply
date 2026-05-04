//! playwright-max-expects oxc backend.

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

fn count_expects_in_source(source: &str, start: usize, end: usize) -> usize {
    let slice = &source[start..end];
    // Count occurrences of "expect(" as a simple heuristic.
    // This matches the tree-sitter approach of walking the subtree.
    let mut count = 0;
    let mut search_from = 0;
    let bytes = slice.as_bytes();
    while search_from + 7 <= bytes.len() {
        if let Some(pos) = slice[search_from..].find("expect(") {
            let abs = search_from + pos;
            // Ensure `expect` is not part of a larger identifier.
            let before_ok =
                abs == 0 || !bytes[abs - 1].is_ascii_alphanumeric() && bytes[abs - 1] != b'_';
            if before_ok {
                count += 1;
            }
            search_from = abs + 7;
        } else {
            break;
        }
    }
    count
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

        let max_expects = ctx.config.threshold("playwright-max-expects", "max", ctx.lang);
        let count = count_expects_in_source(
            ctx.source,
            callback_span.start as usize,
            callback_span.end as usize,
        );
        if count > max_expects {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Too many assertion calls ({count}) — maximum allowed is {max_expects}."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
