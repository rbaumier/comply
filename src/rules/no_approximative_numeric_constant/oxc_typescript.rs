//! no-approximative-numeric-constant — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::cmp::Ordering;
use std::sync::Arc;

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

    let num = num.trim_matches('0');
    for (constant, name) in KNOWN_CONSTS {
        let is_constant_approximated = match constant.len().cmp(&num.len()) {
            Ordering::Less => is_approx_const(num, constant),
            Ordering::Equal => constant == num,
            Ordering::Greater => is_approx_const(constant, num),
        };
        if is_constant_approximated {
            return Some(name);
        }
    }
    None
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
}
