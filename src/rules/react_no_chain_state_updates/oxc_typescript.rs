//! react-no-chain-state-updates OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

/// Count setter-style calls (`setFoo(...)`) in a function body by walking source text.
fn count_setter_calls_in_source(source: &str, start: usize, end: usize) -> usize {
    // Simple approach: find all `setX(` patterns in the body
    let body_text = &source[start..end];
    let mut count = 0;
    let bytes = body_text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if i + 3 < len && &bytes[i..i + 3] == b"set" {
            // Check next char is uppercase
            if i + 3 < len && bytes[i + 3].is_ascii_uppercase() {
                // Find the end of the identifier
                let mut j = i + 4;
                while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                // Skip whitespace then check for `(`
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < len && bytes[j] == b'(' {
                    count += 1;
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    count
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
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

        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "useEffect" {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let body_span = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => {
                Some((arrow.body.span.start as usize, arrow.body.span.end as usize))
            }
            Argument::FunctionExpression(func) => {
                func.body.as_ref().map(|b| (b.span.start as usize, b.span.end as usize))
            }
            _ => None,
        };

        let Some((body_start, body_end)) = body_span else {
            return;
        };

        if body_end > ctx.source.len() {
            return;
        }

        if count_setter_calls_in_source(ctx.source, body_start, body_end) < 2 {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useEffect` chains multiple `setX(...)` calls \u{2014} collapse them into one state object / reducer or derive during render.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
