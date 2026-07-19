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
        Expression::BinaryExpression(bin) => {
            // Allow a bitwise/shift expression between two numeric literals,
            // e.g. `1 << 3`. Both operands are compile-time-constant numbers,
            // so the result is itself a constant the enum inlines — the
            // idiomatic way to declare non-overlapping bit-flag members.
            // Member or identifier references (`A | B`) are not numeric
            // literals, so a computed/aliased initializer stays flagged.
            bin.operator.is_bitwise()
                && matches!(&bin.left, Expression::NumericLiteral(_))
                && matches!(&bin.right, Expression::NumericLiteral(_))
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
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn allows_numeric_bit_shift_const_enum_flags() {
        // Regression for rbaumier/comply#6293 — `1 << N` between numeric
        // literals is a compile-time constant, the idiomatic non-overlapping
        // bit-flag pattern (rollup BitFlags.ts). It must not be flagged.
        let src = r#"
            export const enum Flag {
                included = 1 << 0,
                deoptimized = 1 << 1,
                tdzAccessDefined = 1 << 2,
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_other_numeric_bitwise_operators() {
        let src = r#"
            enum Mask {
                a = 4 | 1,
                b = 6 & 2,
                c = 5 ^ 1,
                d = 8 >> 1,
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_member_reference_bitwise_combination() {
        // Member/identifier references are not numeric literals: aliasing
        // other members (`A | B`) must still be flagged.
        let src = r#"
            enum Flag {
                A = 1,
                B = 2,
                AB = A | B,
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_computed_call_initializer() {
        let src = r#"
            enum Flag {
                computed = computeFlag(),
            }
        "#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn flags_non_bitwise_arithmetic() {
        // Arithmetic (`+`) is outside the bitwise/shift set and stays flagged.
        let src = r#"
            enum N {
                sum = 1 + 2,
            }
        "#;
        assert!(!run(src).is_empty());
    }
}
