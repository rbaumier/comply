//! Shared exemptions for the HTML-sink rules (`no-inner-html`,
//! `no-unsanitized-property`, `no-dynamic-template`).
//!
//! A template literal whose every `${...}` interpolation is provably numeric
//! cannot carry HTML markup — a number serializes to digits, never a `<tag>`.
//! The canonical case is a CSS overlay built from pixel coordinates
//! (`` `left: ${x}px` ``). Any string-typed or unknown interpolation keeps the
//! assignment flagged.

use oxc_ast::ast::{Expression, UnaryOperator};

/// True when `expr` is a `TemplateLiteral` whose every interpolation is provably
/// numeric (see [`is_provably_numeric`]). A template with no interpolations is
/// a static string, handled by the per-rule static-string exemption, so this
/// returns `false` for it (`expressions.is_empty()` short-circuits via `all`).
#[must_use]
pub fn is_numeric_only_template(expr: &Expression) -> bool {
    let Expression::TemplateLiteral(tpl) = expr else {
        return false;
    };
    !tpl.expressions.is_empty() && tpl.expressions.iter().all(is_provably_numeric)
}

/// True when `expr` is structurally numeric, so it cannot inject HTML.
///
/// Covers numeric literals, unary `-`/`+`/`~`, the always-numeric arithmetic
/// operators (`- * / % **` and bitwise), `.length`, and the
/// `Number`/`parseInt`/`parseFloat`/`Math.*` builtins. Binary `+` is only
/// numeric when both operands are provably numeric, since `a + b` is string
/// concatenation when either side is a string. Builtins and `.length` are
/// matched by name against the global environment — a shadowing local binding
/// (`const Number = …`) is not resolved; everything unrecognized stays flagged.
fn is_provably_numeric(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::ParenthesizedExpression(p) => is_provably_numeric(&p.expression),
        Expression::UnaryExpression(u) => match u.operator {
            UnaryOperator::UnaryNegation
            | UnaryOperator::UnaryPlus
            | UnaryOperator::BitwiseNot => is_provably_numeric(&u.argument),
            _ => false,
        },
        Expression::BinaryExpression(bin) => {
            use oxc_ast::ast::BinaryOperator as Op;
            match bin.operator {
                Op::Subtraction
                | Op::Multiplication
                | Op::Division
                | Op::Remainder
                | Op::Exponential
                | Op::BitwiseOR
                | Op::BitwiseAnd
                | Op::BitwiseXOR
                | Op::ShiftLeft
                | Op::ShiftRight
                | Op::ShiftRightZeroFill => true,
                // `a + b` is string concatenation unless both sides are numbers.
                Op::Addition => {
                    is_provably_numeric(&bin.left) && is_provably_numeric(&bin.right)
                }
                _ => false,
            }
        }
        Expression::StaticMemberExpression(member) => member.property.name.as_str() == "length",
        Expression::CallExpression(call) => callee_is_numeric_builtin(&call.callee),
        _ => false,
    }
}

/// True when `callee` is one of the number-producing builtins: `Number(...)`,
/// `parseInt(...)`, `parseFloat(...)`, or any `Math.*(...)` method.
fn callee_is_numeric_builtin(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => {
            matches!(id.name.as_str(), "Number" | "parseInt" | "parseFloat")
        }
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "Math")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    /// Parse `src` and return whether its single expression-statement RHS is a
    /// numeric-only template. `src` must be `x = <expr>;`.
    fn numeric_only(src: &str) -> bool {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, src, SourceType::ts()).parse();
        let stmt = ret.program.body.first().expect("one statement");
        let oxc_ast::ast::Statement::ExpressionStatement(es) = stmt else {
            panic!("expected expression statement");
        };
        let Expression::AssignmentExpression(assign) = &es.expression else {
            panic!("expected assignment");
        };
        is_numeric_only_template(&assign.right)
    }

    #[test]
    fn numeric_literals_only() {
        assert!(numeric_only("x = `left: ${1}px; top: ${2}px`;"));
    }

    #[test]
    fn arithmetic_interpolations() {
        assert!(numeric_only("x = `w: ${10 * 2}px; h: ${a - b}px`;"));
    }

    // `+` is numeric when both operands are provably numeric.
    #[test]
    fn addition_of_two_numbers() {
        assert!(numeric_only("x = `n: ${1 + 2}`;"));
    }

    #[test]
    fn number_builtin() {
        assert!(numeric_only("x = `v: ${Number(s)}; m: ${Math.round(z)}`;"));
    }

    #[test]
    fn length_member() {
        assert!(numeric_only("x = `count: ${items.length}`;"));
    }

    #[test]
    fn bare_identifier_is_not_numeric() {
        // Without type info a bare identifier could be a string.
        assert!(!numeric_only("x = `<b>${userInput}</b>`;"));
    }

    #[test]
    fn string_concat_addition_is_not_numeric() {
        assert!(!numeric_only("x = `<b>${a + name}</b>`;"));
    }

    #[test]
    fn mixed_numeric_and_string_is_not_numeric() {
        assert!(!numeric_only("x = `${1}${userInput}`;"));
    }

    #[test]
    fn static_template_is_not_numeric_only() {
        // No interpolation: handled by the static-string exemption, not this one.
        assert!(!numeric_only("x = `<div></div>`;"));
    }

    #[test]
    fn non_template_rhs_is_not_numeric_only() {
        assert!(!numeric_only("x = userInput;"));
    }
}
