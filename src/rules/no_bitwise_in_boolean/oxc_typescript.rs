//! no-bitwise-in-boolean — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression, UnaryOperator};
use oxc_span::GetSpan;
use std::sync::Arc;

const COMPARISON_OPS: &[BinaryOperator] = &[
    BinaryOperator::Equality,
    BinaryOperator::Inequality,
    BinaryOperator::StrictEquality,
    BinaryOperator::StrictInequality,
    BinaryOperator::LessThan,
    BinaryOperator::GreaterThan,
    BinaryOperator::LessEqualThan,
    BinaryOperator::GreaterEqualThan,
];

/// Whether an identifier name reads as a bit-flag constant (SCREAMING_SNAKE_CASE),
/// e.g. `STATIC_BLOCK`, `BIT`.
fn is_flag_constant_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.chars().any(|c| c.is_ascii_uppercase())
}

/// Whether an identifier name reads as an enum member / flag accessor
/// (PascalCase or SCREAMING_SNAKE_CASE), e.g. `Locations`, `STATIC_BLOCK`.
fn is_flag_member_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Whether an operand is an unambiguous bit-flag signal: a numeric literal,
/// a SCREAMING_SNAKE constant, a member access to an enum-like flag
/// (`ScopeFlag.STATIC_BLOCK`, `OptionFlags.Locations`, `FLAGS.X`), or a
/// bitwise combination of such flags (`ScopeFlag.VAR | ScopeFlag.CLASS_BASE`).
fn is_flag_operand(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(id) => is_flag_constant_name(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            is_flag_member_name(member.property.name.as_str())
        }
        Expression::ParenthesizedExpression(paren) => is_flag_operand(&paren.expression),
        Expression::BinaryExpression(bin)
            if matches!(
                bin.operator,
                BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
            ) =>
        {
            is_flag_operand(&bin.left) && is_flag_operand(&bin.right)
        }
        _ => false,
    }
}

/// Whether a bitwise binary expression is a deliberate bitmask test rather
/// than a likely `&&`/`||` typo. True when either operand is a flag signal,
/// applied recursively so combined masks (`ScopeFlag.VAR | ScopeFlag.CLASS`)
/// remain exempt.
fn is_bitmask_test(bin: &oxc_ast::ast::BinaryExpression) -> bool {
    is_flag_operand(&bin.left) || is_flag_operand(&bin.right)
}

/// Membership-finding methods that return an index (`-1` when absent), making
/// `~call(...)` the classic pre-`Array#includes` "is present?" idiom.
const MEMBERSHIP_FIND_METHODS: &[&str] = &["indexOf", "lastIndexOf", "search"];

/// Whether an expression is `<obj>.indexOf(...)` / `.lastIndexOf(...)` /
/// `.search(...)` — the deliberate `~find()` membership idiom. Unlike a bare
/// `~foo` (a possible `!foo` typo), this `~` is intentional, so it is not a
/// likely logical-operator mistake.
fn is_membership_find_call(expr: &Expression) -> bool {
    let inner = match expr {
        Expression::ParenthesizedExpression(paren) => &paren.expression,
        other => other,
    };
    let Expression::CallExpression(call) = inner else { return false };
    let Expression::StaticMemberExpression(member) = &call.callee else { return false };
    MEMBERSHIP_FIND_METHODS.contains(&member.property.name.as_str())
}

/// Check whether an expression contains a bitwise operator likely standing in
/// for a logical operator. Deliberate bitmask flag tests are not flagged.
fn has_bitwise_op(expr: &Expression) -> bool {
    match expr {
        Expression::BinaryExpression(bin) => {
            if COMPARISON_OPS.contains(&bin.operator) {
                return false;
            }
            if matches!(
                bin.operator,
                BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR
            ) {
                return !is_bitmask_test(bin);
            }
            has_bitwise_op(&bin.left) || has_bitwise_op(&bin.right)
        }
        Expression::UnaryExpression(un) => {
            if un.operator == UnaryOperator::BitwiseNot {
                // `~arr.indexOf(x)` / `~str.search(re)` is the deliberate
                // membership idiom, not a `!foo` typo — leave it unflagged.
                return !is_membership_find_call(&un.argument);
            }
            false
        }
        Expression::ParenthesizedExpression(paren) => has_bitwise_op(&paren.expression),
        _ => false,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement, AstType::WhileStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (test, stmt_span) = match node.kind() {
            oxc_ast::AstKind::IfStatement(s) => (&s.test, s.span()),
            oxc_ast::AstKind::WhileStatement(s) => (&s.test, s.span()),
            _ => return,
        };

        if !has_bitwise_op(test) {
            return;
        }

        let (line, col) = byte_offset_to_line_col(semantic.source_text(), stmt_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: "Bitwise operator in boolean context — did you mean `&&` or `||`?".into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bitwise_and_on_boolean_operands() {
        assert_eq!(run_on("if (isActive & isReady) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_or_on_boolean_operands() {
        assert_eq!(run_on("if (isActive | isReady) {}").len(), 1);
    }

    #[test]
    fn allows_logical_operators() {
        assert!(run_on("if (a && b) {}").is_empty());
        assert!(run_on("if (a || b) {}").is_empty());
    }

    #[test]
    fn allows_comparison_bitmask_test() {
        assert!(run_on("if ((state & FLAG) === 0) {}").is_empty());
        assert!(run_on("while ((mask & bits) !== 0) {}").is_empty());
    }

    #[test]
    fn allows_enum_member_bitmask_test() {
        // Regression for #2064: `if (flags & EnumMember)` is a deliberate bitmask test.
        assert!(run_on("if (flags & ScopeFlag.STATIC_BLOCK) { return true; }").is_empty());
        assert!(run_on("if (optionFlags & OptionFlags.Locations) {}").is_empty());
    }

    #[test]
    fn allows_combined_enum_mask_bitmask_test() {
        assert!(
            run_on("if (flags & (ScopeFlag.VAR | ScopeFlag.CLASS_BASE)) { return false; }")
                .is_empty()
        );
    }

    #[test]
    fn allows_screaming_snake_constant_bitmask_test() {
        assert!(run_on("while (mask & BIT_FLAG) {}").is_empty());
    }

    #[test]
    fn allows_numeric_literal_bitmask_test() {
        assert!(run_on("if (flags & 4) {}").is_empty());
    }

    #[test]
    fn allows_membership_find_idiom() {
        // Regression for #3951: `~find()` is the canonical pre-`includes`
        // membership idiom — unary `~` here is deliberate, not a `!` typo.
        assert!(run_on(r#"if (~program.rawArgs.indexOf("--rename")) {}"#).is_empty());
        assert!(run_on("if (~str.search(/x/)) {}").is_empty());
        assert!(run_on("if (~arr.lastIndexOf(x)) {}").is_empty());
        assert!(run_on("if (~(arr.indexOf(x))) {}").is_empty());
    }

    #[test]
    fn flags_bare_identifier_bitwise_not() {
        // A bare `~foo` is a possible `!foo` typo and must stay flagged.
        assert_eq!(run_on("if (~foo) {}").len(), 1);
        assert_eq!(run_on("if (~someValue) {}").len(), 1);
    }

    #[test]
    fn flags_bitwise_not_on_non_find_member() {
        // Only `.indexOf/.lastIndexOf/.search` *calls* are exempt: a member
        // access that is not such a call stays flagged.
        assert_eq!(run_on("if (~obj.value) {}").len(), 1);
        assert_eq!(run_on("if (~arr.length) {}").len(), 1);
    }
}
