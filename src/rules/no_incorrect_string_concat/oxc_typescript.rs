//! no-incorrect-string-concat OXC backend — flag `"..." + identifier`
//! where the identifier's name suggests it holds a number.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const NUMERIC_HINTS: &[&str] = &[
    "count", "num", "total", "index", "length", "size", "amount", "qty", "sum", "age", "port",
    "offset", "width", "height", "price", "cost",
];

fn looks_numeric(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NUMERIC_HINTS.iter().any(|h| lower.contains(h))
}

fn final_ident_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn is_string_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(_))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != oxc_ast::ast::BinaryOperator::Addition {
            return;
        }

        let flagged = if is_string_literal(&bin.left) {
            final_ident_name(&bin.right).is_some_and(looks_numeric)
        } else if is_string_literal(&bin.right) {
            final_ident_name(&bin.left).is_some_and(looks_numeric)
        } else {
            false
        };

        if flagged {
            let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Suspicious string concatenation with a numeric variable \u{2014} use explicit conversion or template literals.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_string_plus_count() {
        assert_eq!(run_on(r#"const msg = "Total: " + itemCount;"#).len(), 1);
    }


    #[test]
    fn flags_string_plus_total() {
        assert_eq!(run_on(r#"console.log("Sum is " + totalAmount);"#).len(), 1);
    }


    #[test]
    fn allows_string_plus_string_var() {
        assert!(run_on(r#"const msg = "Hello " + userName;"#).is_empty());
    }


    #[test]
    fn allows_template_literal() {
        assert!(run_on(r#"const msg = `Total: ${itemCount}`;"#).is_empty());
    }
}
