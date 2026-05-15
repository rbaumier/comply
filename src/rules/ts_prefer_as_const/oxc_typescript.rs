//! ts-prefer-as-const oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSLiteral, TSType};
use std::sync::Arc;

pub struct Check;

/// True when `expr` is a literal expression that exactly matches the
/// literal type — the only case where `as "X"` is strictly equivalent
/// to `as const`. We don't flag `someVar as "foo"` (the value isn't a
/// literal at this site; the assertion serves a different purpose).
fn expr_matches_literal(expr: &Expression, lit: &TSLiteral) -> bool {
    match (expr, lit) {
        (Expression::StringLiteral(s), TSLiteral::StringLiteral(ts_s)) => {
            s.value.as_str() == ts_s.value.as_str()
        }
        (Expression::NumericLiteral(n), TSLiteral::NumericLiteral(ts_n)) => {
            n.value == ts_n.value
        }
        (Expression::BooleanLiteral(b), TSLiteral::BooleanLiteral(ts_b)) => {
            b.value == ts_b.value
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else {
            return;
        };
        let TSType::TSLiteralType(lit_type) = &as_expr.type_annotation else {
            return;
        };
        if !expr_matches_literal(&as_expr.expression, &lit_type.literal) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Casting a literal to its own literal type adds noise — use \
                      `as const` instead."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_string_literal_cast_to_same_literal() {
        let src = r#"const x = "foo" as "foo";"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_number_literal_cast_to_same_literal() {
        let src = r#"const x = 42 as 42;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_as_const() {
        let src = r#"const x = "foo" as const;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_variable_cast() {
        let src = r#"declare const v: string; const x = v as "foo";"#;
        assert!(run(src).is_empty());
    }
}
