//! prefer-spread — OXC backend.
//! Flags `Array.from()`, `[].concat()`, and `.slice()` / `.slice(0)`.

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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let prop = member.property.name.as_str();

        // Array.from(...) — single iterable arg, not an object literal
        if prop == "from" {
            let Expression::Identifier(obj) = &member.object else { return };
            if obj.name.as_str() != "Array" {
                return;
            }
            if call.arguments.len() >= 2 {
                return;
            }
            if let Some(first) = call.arguments.first() {
                if let Some(Expression::ObjectExpression(_)) = first.as_expression() {
                    return;
                }
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Prefer the spread operator over `Array.from(...)`.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // [].concat(...) — only when receiver is an array literal
        if prop == "concat" {
            if matches!(&member.object, Expression::ArrayExpression(_)) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer the spread operator over `Array#concat(...)`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            return;
        }

        // .slice() or .slice(0) — shallow copy pattern
        if prop == "slice" {
            let is_copy = call.arguments.is_empty()
                || (call.arguments.len() == 1
                    && call.arguments.first().map_or(false, |arg| {
                        matches!(arg.as_expression(), Some(Expression::NumericLiteral(n)) if n.value == 0.0)
                    }));
            if is_copy {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer the spread operator over `Array#slice()`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
    fn flags_array_from() {
        let d = run_on("const arr = Array.from(iterable);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.from"));
    }

    #[test]
    fn flags_concat_array_literal() {
        let d = run_on("const combined = [1,2].concat([3,4]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("concat"));
    }

    #[test]
    fn allows_concat_identifier() {
        assert!(run_on("const combined = arr.concat(other);").is_empty());
    }

    #[test]
    fn allows_array_from_with_map_fn() {
        assert!(run_on("Array.from({ length: 3 }, (_, i) => i);").is_empty());
    }

    #[test]
    fn allows_array_from_object_literal() {
        assert!(run_on("Array.from({ length: 3 });").is_empty());
    }

    #[test]
    fn flags_slice_empty() {
        let d = run_on("const copy = arr.slice();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("slice"));
    }

    #[test]
    fn flags_slice_zero() {
        let d = run_on("const copy = arr.slice(0);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_slice_with_args() {
        assert!(run_on("const sub = arr.slice(1, 3);").is_empty());
    }

    #[test]
    fn allows_spread() {
        assert!(run_on("const arr = [...iterable];").is_empty());
    }
}
