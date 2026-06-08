//! no-unsafe-alloc OXC backend — flag `Buffer.allocUnsafe(...)`,
//! `Buffer.allocUnsafeSlow(...)`, and `new Buffer(size)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Buffer"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                if obj.name.as_str() != "Buffer" {
                    return;
                }
                let prop = member.property.name.as_str();
                if prop != "allocUnsafe" && prop != "allocUnsafeSlow" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`Buffer.{prop}()` returns uninitialized memory — use `Buffer.alloc()` instead."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ctor) = &new_expr.callee else {
                    return;
                };
                if ctor.name.as_str() != "Buffer" {
                    return;
                }
                let Some(first) = new_expr.arguments.first() else {
                    return;
                };
                // Flag numeric args and identifiers (potentially numeric).
                // `new Buffer("string")` or `new Buffer(array)` are not size-based.
                let is_suspect = match first {
                    oxc_ast::ast::Argument::NumericLiteral(_) => true,
                    oxc_ast::ast::Argument::Identifier(_) => true,
                    oxc_ast::ast::Argument::BinaryExpression(_) => true,
                    _ => false,
                };
                if !is_suspect {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`new Buffer(size)` is deprecated and returns uninitialized memory — use `Buffer.alloc(size)` instead.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_buffer_alloc_unsafe() {
        assert_eq!(run_on("const b = Buffer.allocUnsafe(10);").len(), 1);
    }


    #[test]
    fn flags_buffer_alloc_unsafe_slow() {
        assert_eq!(run_on("const b = Buffer.allocUnsafeSlow(10);").len(), 1);
    }


    #[test]
    fn flags_new_buffer_with_size_literal() {
        assert_eq!(run_on("const b = new Buffer(10);").len(), 1);
    }


    #[test]
    fn flags_new_buffer_with_size_variable() {
        assert_eq!(run_on("const b = new Buffer(size);").len(), 1);
    }


    #[test]
    fn allows_buffer_alloc() {
        assert!(run_on("const b = Buffer.alloc(10);").is_empty());
    }


    #[test]
    fn allows_buffer_from() {
        assert!(run_on("const b = Buffer.from('hello');").is_empty());
    }
}
