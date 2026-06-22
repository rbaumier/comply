//! no-zero-fractions oxc backend — flag `1.0`, `2.00`, `3.` number literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, is_typed_array_binding};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpressionElement, AssignmentTarget, BinaryOperator, Expression, ObjectPropertyKind,
};
use std::sync::Arc;

pub struct Check;

/// True when the literal's source text denotes a genuine fractional float —
/// a non-zero fraction (`0.5`, `0.0669873`) or scientific notation (`1e-5`).
/// A pure zero fraction (`1.0`, `0.0`) is NOT genuine: it is the notation this
/// rule normalizes, so it cannot vouch for its siblings.
fn is_genuine_fractional_float(text: &str) -> bool {
    if text.contains('e') || text.contains('E') {
        return true;
    }
    let Some(dot_pos) = text.find('.') else {
        return false;
    };
    text[dot_pos + 1..].chars().any(|c| c != '0' && c != '_')
}

/// Unwrap parentheses to reach the inner expression.
fn unwrap_parens<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap_parens(&p.expression),
        _ => expr,
    }
}

/// True when `expr` is a member access or call on the global `Math` object
/// (`Math.PI`, `Math.log10(x)`).
fn references_math(expr: &Expression) -> bool {
    match unwrap_parens(expr) {
        Expression::StaticMemberExpression(m) => {
            matches!(&m.object, Expression::Identifier(id) if id.name.as_str() == "Math")
        }
        Expression::ComputedMemberExpression(m) => {
            matches!(&m.object, Expression::Identifier(id) if id.name.as_str() == "Math")
        }
        Expression::CallExpression(call) => references_math(&call.callee),
        _ => false,
    }
}

/// True when `operator` is arithmetic (`+ - * / ** %`).
fn is_arithmetic(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Addition
            | BinaryOperator::Subtraction
            | BinaryOperator::Multiplication
            | BinaryOperator::Division
            | BinaryOperator::Exponential
            | BinaryOperator::Remainder
    )
}

/// True when `expr` carries structural evidence of a floating-point computation:
/// a genuine fractional float literal, a `Math.*` reference, or — for nested
/// arithmetic / parentheses — any operand that does. Walks arithmetic operands
/// so `1.0 / (2.0 * Math.PI * fc)` is recognized from its `Math.PI`.
fn is_float_math_expr(expr: &Expression, source: &str) -> bool {
    match unwrap_parens(expr) {
        Expression::NumericLiteral(lit) => {
            is_genuine_fractional_float(&source[lit.span.start as usize..lit.span.end as usize])
        }
        Expression::BinaryExpression(bin) if is_arithmetic(bin.operator) => {
            is_float_math_expr(&bin.left, source) || is_float_math_expr(&bin.right, source)
        }
        other => references_math(other),
    }
}

/// True when the assignment target is a computed element of a `Float32Array`/
/// `Float64Array`-typed binding (`waveform[9] = ...`).
fn assigns_to_typed_array_element(
    target: &AssignmentTarget,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AssignmentTarget::ComputedMemberExpression(member) = target else {
        return false;
    };
    matches!(&member.object, Expression::Identifier(id) if is_typed_array_binding(id, semantic))
}

/// True when `expr` is a genuine fractional float numeric literal (`0.5`, `1e-5`).
fn is_genuine_fractional_literal(expr: &Expression, source: &str) -> bool {
    matches!(
        expr,
        Expression::NumericLiteral(lit)
            if is_genuine_fractional_float(&source[lit.span.start as usize..lit.span.end as usize])
    )
}

/// True when the property at `node` shares an object literal with a property
/// whose value is a genuine fractional float. Mirrors the array-sibling signal
/// for object literals (`{ x: 0.6, y: 7.0 }`): the `.0` is kept for columnar
/// consistency with its fractional sibling. The `N.0` literal itself never
/// matches `is_genuine_fractional_literal`, so no self-exclusion is needed.
fn object_has_fractional_sibling(
    node: &oxc_semantic::AstNode,
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let prop_node = semantic.nodes().parent_node(node.id());
    let AstKind::ObjectExpression(obj) = semantic.nodes().parent_node(prop_node.id()).kind() else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        is_genuine_fractional_literal(&p.value, source)
    })
}

/// True when the `N.0` literal at `node` sits in a floating-point computation.
/// Signals (AST-anchored, never file name / substring):
///   - an element of an array literal that also holds a genuine fractional float
///     (a coefficient/lookup table: `[1.0, 0.5, 0.25, 0.0]`);
///   - a property value of an object literal that also holds a genuine fractional
///     float (`{ x: 0.6, y: 7.0 }`, `{ x: 0.0, h: 0.75 }`);
///   - an operand of an arithmetic `BinaryExpression` whose other side is a
///     genuine fractional float or a `Math.*` term (`20.0 * Math.log10(x)`);
///   - assigned to a `Float32Array`/`Float64Array` element (`buf[i] = 1.0`).
/// A lone `N.0` with none of these (`const count = 1.0`, `setTimeout(fn, 1000.0)`,
/// `arr[1.0]`, `{ x: 1.0, y: 2.0 }`) still flags.
fn is_in_float_math_context(
    node: &oxc_semantic::AstNode,
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let AstKind::NumericLiteral(lit) = node.kind() else {
        return false;
    };
    let lit_span = lit.span;
    let parent = semantic.nodes().parent_node(node.id());

    match parent.kind() {
        AstKind::ArrayExpression(arr) => arr.elements.iter().any(|el| {
            let ArrayExpressionElement::NumericLiteral(sibling) = el else {
                return false;
            };
            sibling.span != lit_span
                && is_genuine_fractional_float(
                    &source[sibling.span.start as usize..sibling.span.end as usize],
                )
        }),
        AstKind::BinaryExpression(bin) if is_arithmetic(bin.operator) => {
            // Inspect the operand that is NOT this literal.
            let other =
                if matches!(&bin.left, Expression::NumericLiteral(l) if l.span == lit_span) {
                    &bin.right
                } else {
                    &bin.left
                };
            is_float_math_expr(other, source)
        }
        AstKind::AssignmentExpression(assign) => {
            assigns_to_typed_array_element(&assign.left, semantic)
        }
        AstKind::ObjectProperty(_) => object_has_fractional_sibling(node, source, semantic),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NumericLiteral(lit) = node.kind() else { return };

        let text = &ctx.source[lit.span.start as usize..lit.span.end as usize];

        // Must contain a dot to be a decimal literal.
        let Some(dot_pos) = text.find('.') else { return };

        // Skip range operator `..` (shouldn't appear in a number node, but guard).
        if text.get(dot_pos + 1..dot_pos + 2) == Some(".") {
            return;
        }

        let fraction = &text[dot_pos + 1..];

        // Dangling dot: `1.` — fraction is empty.
        let is_dangling = fraction.is_empty();

        // Zero fraction: `1.0`, `1.00`, `1.0_0` — fraction is all zeros/underscores.
        let is_zero_fraction =
            !is_dangling && fraction.chars().all(|c| c == '0' || c == '_');

        if !is_dangling && !is_zero_fraction {
            return;
        }

        // A zero-fraction literal inside a floating-point computation (sibling
        // fractional floats, arithmetic with `Math.*`, Float32/64Array element)
        // is intentional float notation, not noise.
        if is_zero_fraction && is_in_float_math_context(node, ctx.source, semantic) {
            return;
        }

        let msg = if is_dangling {
            "Don't use a dangling dot in the number."
        } else {
            "Don't use a zero fraction in the number."
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
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
    fn flags_lone_zero_fraction() {
        let d = run_on("const count = 1.0;");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-zero-fractions");
    }

    #[test]
    fn flags_version_const() {
        assert_eq!(run_on("const VERSION = 2.0;").len(), 1);
    }

    #[test]
    fn flags_settimeout_delay() {
        // A plain call argument has no float-math evidence — still flags.
        assert_eq!(run_on("setTimeout(fn, 1000.0);").len(), 1);
    }

    #[test]
    fn flags_index_access() {
        assert_eq!(run_on("const x = arr[1.0];").len(), 1);
    }

    #[test]
    fn flags_dangling_dot() {
        assert_eq!(run_on("const x = 3.;").len(), 1);
    }

    #[test]
    fn allows_coefficient_array_with_fractional_siblings() {
        // #5408: spectrogram bins — the `0.0` entries share the table's float notation.
        let d = run_on("const expected = [0.0, 0.0669873, 0.9330127, 0.5, 0.0, 0.0];");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_integer_only_array() {
        // No fractional sibling = no float evidence — every `N.0` still flags.
        let d = run_on("const xs = [1.0, 2.0, 3.0];");
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn allows_db_formula_with_math_log10() {
        // #5408: `20.0 * Math.log10(x)` — the decibel multiplier in float math.
        assert!(run_on("const db = 20.0 * Math.log10(x);").is_empty());
    }

    #[test]
    fn allows_nested_arithmetic_with_math_pi() {
        // #5408: `1.0 / (2.0 * Math.PI * fc)` — float evidence reached through nesting.
        assert!(run_on("const k = 1.0 / (2.0 * Math.PI * fc);").is_empty());
    }

    #[test]
    fn allows_arithmetic_with_fractional_float_operand() {
        assert!(run_on("const y = 1.0 + 0.5;").is_empty());
    }

    #[test]
    fn flags_integer_arithmetic() {
        // No float-producing operand — `2.0 * 3` is integer-ish, still flags.
        assert_eq!(run_on("const n = 2.0 * 3;").len(), 1);
    }

    #[test]
    fn allows_float32array_element_assignment() {
        // #5408: impulse signal written into a Float32Array buffer.
        let src = "const waveform = new Float32Array(40); waveform[9] = 1.0;";
        assert!(run_on(src).is_empty(), "got {:?}", run_on(src));
    }

    #[test]
    fn flags_plain_array_element_assignment() {
        // Plain array binding has no float-typed evidence — still flags.
        let src = "const xs = [0, 0, 0]; xs[1] = 1.0;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_object_property_with_fractional_sibling() {
        // #5626: `7.0` keeps `.0` for consistency with `x: 0.6` in the same object.
        let src = r#"const m = { x: 0.6, y: 7.0, color: "FFFFFF", fontSize: 10 };"#;
        assert!(run_on(src).is_empty(), "got {:?}", run_on(src));
    }

    #[test]
    fn allows_object_zero_with_fractional_sibling() {
        // #5626: `0.0` keeps `.0` for consistency with `h: 0.75` in the same object.
        let src = r#"const r = { x: 0.0, y: "90%", w: "100%", h: 0.75 };"#;
        assert!(run_on(src).is_empty(), "got {:?}", run_on(src));
    }

    #[test]
    fn flags_object_with_only_zero_fractions() {
        // No fractional sibling = no float evidence — every `N.0` value still flags.
        let src = "const p = { x: 1.0, y: 2.0 };";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_standalone_object_zero_fraction() {
        // A lone `5.0` property with no fractional sibling still flags.
        let src = r#"const c = { size: 5.0, label: "px" };"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn fractional_sibling_in_nested_object_does_not_leak() {
        // The `0.5` lives in a nested object, not a sibling property of `1.0` —
        // `1.0` has no genuine fractional sibling in its own object, still flags.
        let src = "const o = { a: 1.0, b: { c: 0.5 } };";
        assert_eq!(run_on(src).len(), 1);
    }
}
