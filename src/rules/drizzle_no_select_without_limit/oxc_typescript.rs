//! OxcCheck backend for drizzle-no-select-without-limit.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if a call expression is part of a `.select().from()` chain
/// without `.limit()` or `.where()`.
fn check_select_chain(call: &oxc_ast::ast::CallExpression, source: &str) -> Option<u32> {
    // This call must be `.select(...)`
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    if member.property.name.as_str() != "select" {
        return None;
    }

    // Now we need to check if there's a `.from()` in the chain above us,
    // but in oxc's AST the chain is inverted — we ARE the inner call.
    // The outer calls wrap US. We can't walk up without semantic parent info.
    // So instead, we look at the source text starting from our position
    // to find the chain.

    // Alternative approach: scan a wider window of source after our span
    // to detect `.from(`, `.limit(`, `.where(` in the chain.
    let start = call.span.start as usize;
    // Look at a reasonable window after the select call
    let window_end = (start + 500).min(source.len());
    let window = &source[start..window_end];

    // Find end of the expression statement (semicolon, newline after last paren, etc.)
    let mut depth = 0i32;
    let mut expr_end = window.len();
    let bytes = window.as_bytes();
    let mut past_select = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => {
                depth += 1;
                past_select = true;
            }
            b')' => {
                depth -= 1;
                if past_select && depth == 0 {
                    // Check what follows
                    if i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                        // More chaining, continue
                    } else {
                        expr_end = i + 1;
                        break;
                    }
                }
            }
            b';' | b'\n' if depth <= 0 => {
                expr_end = i;
                break;
            }
            _ => {}
        }
    }

    let chain_text = &window[..expr_end];
    let has_from = chain_text.contains(".from(");
    let has_limit = chain_text.contains(".limit(");
    let has_where = chain_text.contains(".where(");

    if has_from && !has_limit && !has_where {
        Some(call.span.start)
    } else {
        None
    }
}

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        if let Some(span_start) = check_select_chain(call, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`db.select().from(table)` without `.limit()` or `.where()` scans the \
                          entire table — add a bound to avoid loading unbounded rows."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
