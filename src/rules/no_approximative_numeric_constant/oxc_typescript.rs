//! no-approximative-numeric-constant — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::cmp::Ordering;
use std::sync::Arc;

/// Constructors whose array argument holds precomputed binary data, never a
/// symbolic math constant.
const TYPED_ARRAY_CTORS: [&str; 11] = [
    "Float32Array",
    "Float64Array",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "BigInt64Array",
    "BigUint64Array",
];

/// Standard `Math` constants and their decimal representations, ordered as in
/// Biome's source. Fraction-only constants keep their leading `.` (no `0`).
const KNOWN_CONSTS: [(&str, &str); 8] = [
    ("2.718281828459045", "E"),
    ("2.302585092994046", "LN10"),
    (".6931471805599453", "LN2"),
    (".4342944819032518", "LOG10E"),
    ("1.4426950408889634", "LOG2E"),
    ("3.141592653589793", "PI"),
    (".7071067811865476", "SQRT1_2"),
    ("1.4142135623730951", "SQRT2"),
];

/// A literal must carry at least this many fractional digits to be considered
/// an approximation worth flagging; shorter ones are too ambiguous.
const MIN_FRACTION_DIGITS: usize = 3;

/// Returns the `(radix, digits)` for a JS numeric prefix, or `None` for a plain
/// decimal. Mirrors `biome_js_syntax::numbers::parse_js_number_prefix`.
fn parse_js_number_prefix(num: &str) -> Option<(u8, &str)> {
    let mut bytes = num.bytes();
    if bytes.next()? != b'0' {
        return None;
    }
    Some(match bytes.next()? {
        b'x' | b'X' => (16, &num[2..]),
        b'o' | b'O' => (8, &num[2..]),
        b'b' | b'B' => (2, &num[2..]),
        // Legacy octal literals (leading `0` followed only by digits 0-7).
        b'0'..=b'7' if bytes.all(|b| !matches!(b, b'8' | b'9')) => (8, &num[1..]),
        _ => return None,
    })
}

/// Whether `value` is a leading-digit approximation of `constant` (either a
/// straight truncation or a correctly rounded truncation).
fn is_approx_const(constant: &str, value: &str) -> bool {
    if constant.starts_with(value) {
        // The value is a truncated constant.
        return true;
    }
    let (digits, last_digit) = value.split_at(value.len() - 1);
    if constant.starts_with(digits) {
        let Ok(last_digit) = last_digit.parse::<u8>() else {
            return false;
        };
        let Ok(extra_constant_digit) = constant[value.len()..value.len() + 1].parse::<u8>() else {
            return false;
        };
        let can_be_rounded = extra_constant_digit < 5;
        if can_be_rounded {
            return false;
        }
        let Ok(constant_digit) = constant[digits.len()..digits.len() + 1].parse::<u8>() else {
            return false;
        };
        let rounded_constant_digit = constant_digit + 1;
        return last_digit == rounded_constant_digit;
    }
    false
}

/// Returns the matching `Math` constant name if `raw` (the literal's printed
/// text) approximates one. Mirrors Biome's `noApproximativeNumericConstant`.
fn approximated_constant(raw: &str) -> Option<&'static str> {
    let (radix, num) = parse_js_number_prefix(raw).unwrap_or((10, raw));
    if radix != 10 {
        return None;
    }
    let stripped;
    let num = if num.contains('_') {
        stripped = num.replace('_', "");
        stripped.as_str()
    } else {
        num
    };

    let (decimal, fraction) = num.split_once('.')?;
    if fraction.len() < MIN_FRACTION_DIGITS
        || !matches!(decimal, "" | "0" | "1" | "2" | "3")
        || fraction.contains(['e', 'E'])
    {
        return None;
    }

    // Normalize a bare-zero integer part (`0.693` → `.693`) so it lines up with
    // the fraction-only constants, which omit the leading `0`. Trailing zeros in
    // the fraction stay: they are significant, so `0.690` must not collapse into
    // a prefix of `.6931…` (LN2).
    let normalized = if decimal == "0" {
        &num[1..]
    } else {
        num
    };
    for (constant, name) in KNOWN_CONSTS {
        let is_constant_approximated = match constant.len().cmp(&normalized.len()) {
            Ordering::Less => is_approx_const(normalized, constant),
            Ordering::Equal => constant == normalized,
            Ordering::Greater => is_approx_const(constant, normalized),
        };
        if is_constant_approximated {
            return Some(name);
        }
    }
    None
}

/// Counts the numeric elements (positive or unary-negated numeric literals) of
/// an array literal. Spread/object/string elements are ignored, so a mostly
/// numeric data array still reads as numeric.
fn numeric_element_count(arr: &oxc_ast::ast::ArrayExpression) -> usize {
    arr.elements
        .iter()
        .filter(|elem| match elem.as_expression() {
            Some(Expression::NumericLiteral(_)) => true,
            Some(Expression::UnaryExpression(unary)) => {
                matches!(unary.argument, Expression::NumericLiteral(_))
            }
            _ => false,
        })
        .count()
}

/// Whether the literal `node` sits in a numeric *data* array — a typed-array
/// initializer (`new Float32Array([...])`, etc.) or a plain array literal that
/// holds at least `min_array_elements` numeric siblings. Such a literal is
/// precomputed data, so a coincidental Math-constant prefix match is spurious.
fn is_in_numeric_data_array(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
    min_array_elements: usize,
) -> bool {
    let nodes = semantic.nodes();

    // A negated literal (`-0.985`) is wrapped in a UnaryExpression; step over it
    // so the array literal is the immediate parent we inspect.
    let mut current = node.id();
    if matches!(nodes.parent_node(current).kind(), oxc_ast::AstKind::UnaryExpression(_)) {
        current = nodes.parent_node(current).id();
    }

    let parent = nodes.parent_node(current);
    let oxc_ast::AstKind::ArrayExpression(arr) = parent.kind() else {
        return false;
    };

    // Typed-array initializer: `new Float32Array([...])`. Walk past any nesting
    // of array literals (matrix-of-rows) up to the enclosing `new` expression.
    let mut ancestor = parent.id();
    loop {
        let grandparent = nodes.parent_node(ancestor);
        match grandparent.kind() {
            oxc_ast::AstKind::ArrayExpression(_) => ancestor = grandparent.id(),
            oxc_ast::AstKind::NewExpression(new_expr) => {
                if let Expression::Identifier(id) = &new_expr.callee
                    && TYPED_ARRAY_CTORS.contains(&id.name.as_str())
                {
                    return true;
                }
                break;
            }
            _ => break,
        }
    }

    numeric_element_count(arr) >= min_array_elements
}

#[derive(Debug)]
pub struct Check;

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
        let oxc_ast::AstKind::NumericLiteral(lit) = node.kind() else {
            return;
        };
        let span = lit.span();
        let raw = &semantic.source_text()[span.start as usize..span.end as usize];
        if let Some(name) = approximated_constant(raw) {
            let min_array_elements =
                ctx.config
                    .threshold("no-approximative-numeric-constant", "min_array_elements", ctx.lang);
            if is_in_numeric_data_array(node, semantic, min_array_elements) {
                return;
            }
            let (line, col) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Prefer the standard constant `Math.{}` over the approximated literal `{}`.",
                    name, raw
                ),
                severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Test-dir gate (rbaumier/comply#5273) ---

    // maplibre/maplibre-gl-js — test files hardcode truncated/rounded constant
    // values as deliberate assertion boundaries: `1.4142`/`1.4143` straddle the
    // `sqrt(2)` threshold (one yields Partial, the other Full), and `0.707` is
    // the rounded output of `Math.round(n * 1000) / 1000`. Swapping in the
    // symbolic `Math` constant would change the test's precision intent, so the
    // central `skip_in_test_dir` gate suppresses the rule in test files.
    #[test]
    fn gated_no_fp_on_truncated_boundary_in_test_file() {
        use crate::rules::test_helpers::run_rule_gated;
        let src =
            "expect(obb.intersectsPlane([1, 0, 0, 1.4142])).toBe(IntersectionResult.Partial);\n";
        assert!(
            run_rule_gated(&Check, src, "src/util/primitives/convex_volume.test.ts").is_empty(),
            "skip_in_test_dir must suppress approximations in test files"
        );
    }

    #[test]
    fn gated_no_fp_on_rounded_expected_value_in_test_file() {
        use crate::rules::test_helpers::run_rule_gated;
        let src = "const expectedFrustumPlanes = [[-0.707, 0, 0.707, -0]];\n";
        assert!(
            run_rule_gated(&Check, src, "src/util/primitives/frustum.test.ts").is_empty(),
            "skip_in_test_dir must suppress rounded expected values in test files"
        );
    }

    // juliangarnier/anime tests/suites/parameters.test.js — a high-precision
    // literal used as an animation *target value* (`{ plainValue: 3.14159265359 }`)
    // and re-asserted with `expect(...).to.equal(3.14159265359)`. The point of the
    // test is that the engine preserves an arbitrary float; rewriting it to
    // `Math.PI` would change the test's intent. The path lives under `tests/`, so
    // the `skip_in_test_dir` gate suppresses every literal in the file.
    #[test]
    fn gated_no_fp_on_animation_target_value_in_test_file() {
        use crate::rules::test_helpers::run_rule_gated;
        let src = "const animation1 = animate(testObject, { plainValue: 3.14159265359 });\n\
            expect(testObject.plainValue).to.equal(3.14159265359);\n";
        assert!(
            run_rule_gated(&Check, src, "tests/suites/parameters.test.js").is_empty(),
            "skip_in_test_dir must suppress animation target values in test files"
        );
    }

    // A symbolic-constant approximation in a production source file is a real
    // approximation and must keep firing.
    #[test]
    fn gated_still_flags_approximation_in_production() {
        use crate::rules::test_helpers::run_rule_gated;
        // The same high-precision literal from #5307, but in production code, is a
        // genuine π approximation: outside a test the gate does not apply.
        for src in ["const x = 3.14159;\n", "const x = 3.14159265359;\n"] {
            let d = run_rule_gated(&Check, src, "src/util/math.ts");
            assert_eq!(d.len(), 1, "production constant approximation must still be flagged: {src}");
            assert!(d[0].message.contains("Math.PI"));
        }
    }

    // --- Biome valid.js fixtures: must NOT fire ---

    #[test]
    fn allows_math_constant_references() {
        assert!(run("const x = Math.PI;").is_empty());
        assert!(run("const y = Math.LN10;").is_empty());
    }

    #[test]
    fn allows_rounded_away_neighbours() {
        // Close to LN10 (2.302585…) but the next digit rounds the wrong way.
        assert!(run("const z = 2.301;").is_empty());
        assert!(run("const w = 2.304;").is_empty());
    }

    #[test]
    fn allows_too_few_fraction_digits() {
        // 3.14 only has two fractional digits — below MIN_FRACTION_DIGITS.
        assert!(run("const piOk = 3.14;").is_empty());
    }

    // --- Biome invalid.js fixtures: must fire with the right constant ---

    #[test]
    fn flags_truncated_pi() {
        let d = run("const x = 3.141;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.PI"));
    }

    #[test]
    fn flags_truncated_ln10() {
        let d = run("const y = 2.302;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.LN10"));
    }

    #[test]
    fn flags_longer_ln10_approximation() {
        let d = run("const z = 2.3025;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.LN10"));
    }

    #[test]
    fn flags_six_digit_approximations() {
        for (src, name) in [
            ("const v = 2.718281;", "Math.E"),
            ("const v = 2.302585;", "Math.LN10"),
            ("const v = 0.693147;", "Math.LN2"),
            ("const v = 0.434294;", "Math.LOG10E"),
            ("const v = 1.442695;", "Math.LOG2E"),
            ("const v = 3.141592;", "Math.PI"),
            ("const v = 0.707106;", "Math.SQRT1_2"),
            ("const v = 1.414213;", "Math.SQRT2"),
        ] {
            let d = run(src);
            assert_eq!(d.len(), 1, "expected one diagnostic for {src}");
            assert!(d[0].message.contains(name), "{src} should suggest {name}");
        }
    }

    #[test]
    fn flags_three_digit_approximations() {
        for (src, name) in [
            ("const v = 2.718;", "Math.E"),
            ("const v = 2.302;", "Math.LN10"),
            ("const v = 0.693;", "Math.LN2"),
            ("const v = 0.434;", "Math.LOG10E"),
            ("const v = 1.442;", "Math.LOG2E"),
            ("const v = 3.141;", "Math.PI"),
            ("const v = 0.707;", "Math.SQRT1_2"),
            ("const v = 1.414;", "Math.SQRT2"),
        ] {
            let d = run(src);
            assert_eq!(d.len(), 1, "expected one diagnostic for {src}");
            assert!(d[0].message.contains(name), "{src} should suggest {name}");
        }
    }

    #[test]
    fn flags_e_with_numeric_separator() {
        // Underscore separators are stripped before matching.
        let d = run("const v = 2.718_281;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.E"));
    }

    // --- Over-firing guards ---

    #[test]
    fn ignores_unrelated_numbers() {
        assert!(run("const x = 1234;").is_empty());
        assert!(run("const x = 9.999;").is_empty());
        assert!(run("const x = 42.5;").is_empty());
    }

    #[test]
    fn ignores_non_decimal_radix() {
        assert!(run("const x = 0xff;").is_empty());
        assert!(run("const x = 0b101;").is_empty());
        assert!(run("const x = 0o777;").is_empty());
    }

    #[test]
    fn ignores_large_integer_part() {
        // Integer part outside {"", 0, 1, 2, 3}, so never an approximation.
        assert!(run("const x = 4.141592;").is_empty());
    }

    #[test]
    fn ignores_trailing_zero_literals() {
        // Regression for #5245: trailing zeros are significant. These are not
        // approximations of any Math constant and must not be flagged.
        assert!(run("const z = 0.000000;").is_empty(), "0.000000 is zero, not LN2");
        assert!(run("const z = 0.0000000;").is_empty(), "0.0000000 is zero, not LN2");
        assert!(run("const p = 0.690;").is_empty(), "0.690 rounds to .69, not LN2 (.6931)");
        assert!(run("const p = 0.600;").is_empty(), "0.600 rounds to .6, not LN2");
    }

    #[test]
    fn flags_genuine_approximations_with_enough_precision() {
        // Counterpart to the trailing-zero guard: real approximations still fire.
        for (src, name) in [
            ("const v = 0.6931;", "Math.LN2"),
            ("const v = 0.69314;", "Math.LN2"),
            ("const v = 0.7071;", "Math.SQRT1_2"),
            ("const v = 3.14159;", "Math.PI"),
            ("const v = 2.7182;", "Math.E"),
        ] {
            let d = run(src);
            assert_eq!(d.len(), 1, "expected one diagnostic for {src}");
            assert!(d[0].message.contains(name), "{src} should suggest {name}");
        }
    }

    // --- #5266: numeric data-array context ---

    #[test]
    fn ignores_constant_match_in_typed_array_initializer() {
        // Regression for #5266: a body-tracking rotation-matrix coefficient that
        // coincidentally matches LOG10E's prefix is data, not a symbolic constant.
        let src = "const SnapshotRhs = new Float32Array([\n\
            -0.036, -0.985, -0.168, 0, -0.113, 0.171, -0.979, 0, 0.993, -0.016, -0.117, 0,\n\
            0.006, 0.586, 2.035, 1,\n\
            0.434, 0.775, 0.46, 0,\n\
        ]);";
        assert!(run(src).is_empty(), "typed-array data element must not be flagged");
    }

    #[test]
    fn ignores_constant_match_in_small_typed_array() {
        // Typed arrays are always data regardless of element count.
        assert!(run("const v = new Float64Array([0.434, 1.0]);").is_empty());
    }

    #[test]
    fn ignores_constant_match_in_large_numeric_array() {
        // A plain array literal with many numeric siblings is a data table.
        let src = "const data = [0.1, 0.2, 0.3, 0.434, 0.5, 0.6, 0.7, 0.8, 0.9];";
        assert!(run(src).is_empty(), "element of a large numeric array is data");
    }

    #[test]
    fn ignores_constant_match_in_numeric_matrix() {
        // Nested matrix rows inside a typed array are still data.
        let src = "const m = new Float32Array([[0.434, 0.5], [0.6, 0.7]]);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_constant_in_small_array() {
        // A small array tuple where the value is plausibly the symbolic constant.
        let d = run("const ratios = [0.707, 1.0];");
        assert_eq!(d.len(), 1, "small array element is still a symbolic use");
        assert!(d[0].message.contains("Math.SQRT1_2"));
    }

    #[test]
    fn flags_standalone_symbolic_constant() {
        // The classic symbolic use is unaffected by the data-array gate.
        let d = run("const x = 3.14159;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.PI"));
    }

    #[test]
    fn flags_standalone_negated_symbolic_constant() {
        // Stepping over the negation must not suppress a standalone constant.
        let d = run("const x = -3.14159;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.PI"));
    }
}
