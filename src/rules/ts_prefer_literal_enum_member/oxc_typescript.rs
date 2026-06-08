//! ts-prefer-literal-enum-member OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_literal(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => {
            // Only literal if no expressions.
            tpl.expressions.is_empty()
        }
        Expression::UnaryExpression(unary) => {
            // Allow +N and -N.
            matches!(
                unary.operator,
                oxc_ast::ast::UnaryOperator::UnaryPlus
                    | oxc_ast::ast::UnaryOperator::UnaryNegation
            ) && matches!(&unary.argument, Expression::NumericLiteral(_))
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSEnumDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["enum"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSEnumDeclaration(decl) = node.kind() else { return };

        for member in &decl.body.members {
            let Some(ref init) = member.initializer else {
                // No initializer — auto-increment, that's fine.
                continue;
            };
            if is_literal(init) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, member.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Enum member should be initialized with a literal value \
                          (string or number), not a computed expression."
                    .into(),
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
    fn allows_string_literal() {
        assert!(run_on(r#"enum E { A = "hello" }"#).is_empty());
    }


    #[test]
    fn allows_number_literal() {
        assert!(run_on("enum E { A = 1 }").is_empty());
    }


    #[test]
    fn allows_no_initializer() {
        assert!(run_on("enum E { A, B, C }").is_empty());
    }


    #[test]
    fn flags_computed_expression() {
        let diags = run_on("enum E { A = getValue() }");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_reference_to_variable() {
        let diags = run_on("const x = 1; enum E { A = x }");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_negative_number() {
        assert!(run_on("enum E { A = -1 }").is_empty());
    }
}
