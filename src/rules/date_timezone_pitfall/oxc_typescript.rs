//! date-timezone-pitfall oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, CallExpression, Expression};
use std::sync::Arc;

pub struct Check;

/// `true` when `s` is exactly `YYYY-MM-DD` — four digits, hyphen, two digits,
/// hyphen, two digits, nothing else. This is the date-only form the ECMAScript
/// Date Time String Format parses as **UTC** midnight, so in a non-UTC zone it
/// resolves to the previous or next local day. A string that carries any time
/// component (`T`, `Z`, `:`) is a full instant and is parsed unambiguously, so
/// it must not match here.
fn is_date_only_string(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 {
        return false;
    }
    b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit()
}

/// A template literal stands in for a date-only string when its static skeleton —
/// the quasis with every `${…}` interpolation replaced by a single digit — has the
/// shape `digits-digits-digits`, carries no time component, AND has at least one
/// literal digit in its static text. The relaxed digit bounds (1–4 / 1–2) absorb
/// interpolations standing in for a year, month, or day; the `T`/`Z`/`:` exclusion
/// (already implied by the shape) keeps full ISO templates such as
/// `` `${base}T00:00:00` `` from matching. Requiring a literal digit rejects a
/// fully-interpolated composite key like `` `${a}-${b}-${c}` `` (skeleton `0-0-0`),
/// which is a hyphen-joined identifier, not a date.
fn is_date_only_template_skeleton(skeleton: &str, has_literal_digit: bool) -> bool {
    if !has_literal_digit {
        return false;
    }
    let mut parts = skeleton.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let digits = |p: &str, lo: usize, hi: usize| {
        let n = p.len();
        n >= lo && n <= hi && p.bytes().all(|c| c.is_ascii_digit())
    };
    digits(year, 1, 4) && digits(month, 1, 2) && digits(day, 1, 2)
}

/// Build the static skeleton of a template literal — each static quasi verbatim,
/// each `${…}` interpolation replaced by a single `0` placeholder — and report
/// whether any static quasi contributed a literal ASCII digit.
fn template_skeleton(tpl: &oxc_ast::ast::TemplateLiteral) -> (String, bool) {
    let mut skeleton = String::new();
    let mut has_literal_digit = false;
    for (i, quasi) in tpl.quasis.iter().enumerate() {
        if i > 0 {
            skeleton.push('0');
        }
        let raw = quasi.value.raw.as_str();
        has_literal_digit |= raw.bytes().any(|c| c.is_ascii_digit());
        skeleton.push_str(raw);
    }
    (skeleton, has_literal_digit)
}

/// (a) `new Date(arg)` where `arg` is a date-only string literal or template.
fn flag_new_date(new_expr: &oxc_ast::ast::NewExpression) -> bool {
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    if callee.name.as_str() != "Date" {
        return false;
    }
    let Some(arg) = new_expr.arguments.first().and_then(Argument::as_expression) else {
        return false;
    };
    match arg {
        Expression::StringLiteral(lit) => is_date_only_string(lit.value.as_str()),
        Expression::TemplateLiteral(tpl) => {
            let (skeleton, has_literal_digit) = template_skeleton(tpl);
            is_date_only_template_skeleton(&skeleton, has_literal_digit)
        }
        _ => false,
    }
}

/// `true` when `expr` is a `*.toISOString()` call.
fn is_to_iso_string_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    if !call.arguments.is_empty() {
        return false;
    }
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "toISOString"
}

/// `true` when `n` is the literal integer `value`.
fn is_int_literal(arg: Option<&Argument>, value: f64) -> bool {
    matches!(arg, Some(Argument::NumericLiteral(lit)) if lit.value == value)
}

/// (b1) `<toISOString()>.slice(0, 10)` / `.substring(0, 10)`.
fn is_iso_slice_truncation(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "slice" | "substring") {
        return false;
    }
    if !is_to_iso_string_call(&member.object) {
        return false;
    }
    call.arguments.len() == 2
        && is_int_literal(call.arguments.first(), 0.0)
        && is_int_literal(call.arguments.get(1), 10.0)
}

/// `true` when `call` is `<toISOString()>.split("T")`.
fn is_iso_split_on_t(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "split" {
        return false;
    }
    if !is_to_iso_string_call(&member.object) {
        return false;
    }
    matches!(
        call.arguments.first().and_then(Argument::as_expression),
        Some(Expression::StringLiteral(lit)) if lit.value.as_str() == "T"
    )
}

/// (b2) `<toISOString()>.split("T")[0]`.
fn is_iso_split_truncation(member: &oxc_ast::ast::ComputedMemberExpression) -> bool {
    let Expression::CallExpression(call) = &member.object else {
        return false;
    };
    if !is_iso_split_on_t(call) {
        return false;
    }
    matches!(&member.expression, Expression::NumericLiteral(lit) if lit.value == 0.0)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression, AstType::CallExpression, AstType::ComputedMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Date", "toISOString"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (offset, message) = match node.kind() {
            AstKind::NewExpression(new_expr) if flag_new_date(new_expr) => (
                new_expr.span.start as usize,
                "`new Date(\"YYYY-MM-DD\")` parses the date-only string as UTC midnight, \
                 shifting the calendar day in non-UTC zones.",
            ),
            AstKind::CallExpression(call) if is_iso_slice_truncation(call) => (
                call.span.start as usize,
                "Truncating a `toISOString()` result converts to UTC first, shifting the \
                 calendar day in non-UTC zones.",
            ),
            AstKind::ComputedMemberExpression(member) if is_iso_split_truncation(member) => (
                member.span.start as usize,
                "Truncating a `toISOString()` result converts to UTC first, shifting the \
                 calendar day in non-UTC zones.",
            ),
            _ => return,
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: message.into(),
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

    // (a) date-only `new Date(...)` — bad.
    #[test]
    fn flags_date_only_string_literal() {
        let d = run_on(r#"new Date("2026-01-15");"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "date-timezone-pitfall");
    }

    #[test]
    fn flags_date_only_template() {
        assert_eq!(run_on("const y = 2026; new Date(`${y}-01-15`);").len(), 1);
    }

    // (b) toISOString truncation — bad.
    #[test]
    fn flags_to_iso_string_slice() {
        assert_eq!(run_on("d.toISOString().slice(0, 10);").len(), 1);
    }

    #[test]
    fn flags_to_iso_string_substring() {
        assert_eq!(run_on("d.toISOString().substring(0, 10);").len(), 1);
    }

    #[test]
    fn flags_to_iso_string_split() {
        assert_eq!(run_on(r#"d.toISOString().split("T")[0];"#).len(), 1);
    }

    // (a) guardrails — good.
    #[test]
    fn allows_full_iso_datetime_string() {
        assert!(run_on(r#"new Date("2026-01-15T10:00:00Z");"#).is_empty());
    }

    #[test]
    fn allows_new_date_with_variable() {
        assert!(run_on("new Date(someVar);").is_empty());
    }

    #[test]
    fn allows_new_date_with_numeric_components() {
        assert!(run_on("new Date(2026, 0, 15);").is_empty());
    }

    #[test]
    fn allows_non_date_shaped_string() {
        assert!(run_on(r#"new Date("hello world");"#).is_empty());
    }

    #[test]
    fn allows_full_iso_template() {
        assert!(run_on("const base = '2026-01-15'; new Date(`${base}T00:00:00`);").is_empty());
    }

    // A fully-interpolated hyphen-joined composite key has the `0-0-0` skeleton
    // but no literal digit, so it is an identifier, not a date — must not fire.
    #[test]
    fn allows_fully_interpolated_composite_template() {
        assert!(run_on("new Date(`${order}-${item}-${qty}`);").is_empty());
    }

    // (b) guardrails — good.
    #[test]
    fn allows_bare_to_iso_string() {
        assert!(run_on("const s = d.toISOString();").is_empty());
    }

    #[test]
    fn allows_slice_on_non_iso_string() {
        assert!(run_on(r#"someString.slice(0, 10);"#).is_empty());
    }

    #[test]
    fn allows_split_on_non_iso_string() {
        assert!(run_on(r#"someString.split("T")[0];"#).is_empty());
    }

    #[test]
    fn allows_iso_slice_with_other_bounds() {
        assert!(run_on("d.toISOString().slice(0, 19);").is_empty());
    }
}
