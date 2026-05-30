//! OXC backend for testing-no-concurrent-without-context-expect.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression};
use std::sync::Arc;

pub struct Check;

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn contains_identifier(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + n.len();
            let after_ok = after_idx == bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `test.concurrent` or `it.concurrent`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "concurrent" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if !matches!(obj.name.as_str(), "test" | "it") {
            return;
        }

        // Find the callback argument (arrow function or function expression).
        let Some(callback_expr) = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                    Some(expr)
                }
                _ => None,
            }
        }) else {
            return;
        };

        let (params, fn_span) = match callback_expr {
            Expression::ArrowFunctionExpression(f) => (&f.params, f.span),
            Expression::FunctionExpression(f) => (&f.params, f.span),
            _ => return,
        };

        // Only flag when `expect` is actually referenced in the callback.
        // Callbacks that delegate to an external function without calling expect
        // directly (e.g. test-utility wrappers like txTest) are safe — the global
        // expect is only problematic for snapshot matchers, not regular assertions.
        let fn_text = &ctx.source[fn_span.start as usize..fn_span.end as usize];
        if !contains_identifier(fn_text, "expect") {
            return;
        }

        // Check if the first parameter destructures `expect`.
        let has_expect = params.items.first().is_some_and(|param| {
            if let BindingPattern::ObjectPattern(obj_pat) = &param.pattern {
                obj_pat.properties.iter().any(|prop| {
                    let key_name = prop.key.name();
                    key_name.as_deref() == Some("expect")
                })
            } else {
                false
            }
        });

        if !has_expect {
            let span = params.span;
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "test.concurrent must destructure { expect } from the test context — the module-level expect is not scoped per concurrent test.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
