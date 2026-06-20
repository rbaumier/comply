//! date-timezone-pitfall oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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

/// `new Date(arg)` where `arg` is a date-only string literal or template.
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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Date"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        if !flag_new_date(new_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new Date(\"YYYY-MM-DD\")` parses the date-only string as UTC midnight, \
                      shifting the calendar day in non-UTC zones."
                .into(),
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

    // date-only `new Date(...)` — bad.
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

    // `toISOString()` is always UTC, so truncating its date part is deterministic
    // regardless of how the Date was built — it is the idiomatic, correct way to
    // obtain a UTC date string and must never be flagged.
    #[test]
    fn allows_to_iso_string_slice() {
        assert!(run_on("d.toISOString().slice(0, 10);").is_empty());
    }

    #[test]
    fn allows_to_iso_string_substring() {
        assert!(run_on("d.toISOString().substring(0, 10);").is_empty());
    }

    #[test]
    fn allows_to_iso_string_split() {
        assert!(run_on(r#"d.toISOString().split("T")[0];"#).is_empty());
    }

    // Issue #4567: a UTC-anchored Date round-tripped through
    // `toISOString().slice(0, 10)` to validate a calendar date — drift-free.
    #[test]
    fn allows_utc_anchored_round_trip_reproducer() {
        let src = r#"
            schema.refine((isoDate) => {
              const candidate = new Date(`${isoDate}T00:00:00Z`);
              return !Number.isNaN(candidate.getTime())
                && candidate.toISOString().slice(0, 10) === isoDate;
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    // guardrails — good.
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
}
