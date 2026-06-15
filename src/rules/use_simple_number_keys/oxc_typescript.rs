//! use-simple-number-keys OXC backend.
//!
//! Flags numeric object member names that are not a plain base-10 number:
//! BigInt (`1n`), hexadecimal (`0x1`), binary (`0b1`), octal (`0o1` and legacy
//! `01`), and any decimal/float using an underscore separator (`1_0`,
//! `0.1e1_2`). Computed keys (`{ [0x1]: 1 }`) are dynamic expressions, not
//! literal member names, and are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// The forbidden numeric-key forms, paired with their diagnostic message.
#[derive(Clone, Copy)]
enum WrongNumberKey {
    BigInt,
    Hexadecimal,
    Binary,
    Octal,
    Underscore,
}

impl WrongNumberKey {
    fn message(self) -> &'static str {
        match self {
            Self::BigInt => "Bigint is not allowed here.",
            Self::Hexadecimal => "Hexadecimal number literal is not allowed here.",
            Self::Binary => "Binary number literal in is not allowed here.",
            Self::Octal => "Octal number literal is not allowed here.",
            Self::Underscore => "Number literal with underscore is not allowed here.",
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        // Computed keys (`{ [0x1]: 1 }`) are dynamic, not literal member names.
        if prop.computed {
            return;
        }
        let Some(wrong) = classify_key(&prop.key, ctx.source) else {
            return;
        };
        let span = prop.key.span();
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "use-simple-number-keys".into(),
            message: wrong.message().into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Classify a numeric member key into its forbidden form, or `None` when the
/// key is not a numeric literal or is a plain base-10 number.
fn classify_key(key: &PropertyKey, source: &str) -> Option<WrongNumberKey> {
    match key {
        PropertyKey::BigIntLiteral(_) => Some(WrongNumberKey::BigInt),
        PropertyKey::NumericLiteral(lit) => {
            let span = lit.span;
            let raw = source.get(span.start as usize..span.end as usize)?;
            classify_number_literal(raw)
        }
        _ => None,
    }
}

/// Classify the raw source text of a numeric literal. Prefix forms take
/// priority over the underscore check so `0x1_0` reports as hexadecimal,
/// matching Biome.
fn classify_number_literal(raw: &str) -> Option<WrongNumberKey> {
    let bytes = raw.as_bytes();
    if let [b'0', b'x' | b'X', ..] = bytes {
        return Some(WrongNumberKey::Hexadecimal);
    }
    if let [b'0', b'b' | b'B', ..] = bytes {
        return Some(WrongNumberKey::Binary);
    }
    if let [b'0', b'o' | b'O', ..] = bytes {
        return Some(WrongNumberKey::Octal);
    }
    if raw.contains('_') {
        return Some(WrongNumberKey::Underscore);
    }
    if is_legacy_octal(raw) {
        return Some(WrongNumberKey::Octal);
    }
    None
}

/// A legacy octal literal: a leading `0` followed by one or more octal digits
/// (`01`, `0123`). `0` alone is plain decimal zero; `08`/`09` and any literal
/// with a `.` or exponent are decimal, not octal.
fn is_legacy_octal(raw: &str) -> bool {
    let mut chars = raw.chars();
    if chars.next() != Some('0') {
        return false;
    }
    let rest = chars.as_str();
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() && b < b'8')
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

    fn messages(source: &str) -> Vec<String> {
        run_on(source).into_iter().map(|d| d.message).collect()
    }

    // ── Biome invalid.js fixtures ─────────────────────────────────────────

    #[test]
    fn flags_bigint_key() {
        assert_eq!(messages("({ 1n: 1 });"), vec!["Bigint is not allowed here."]);
    }

    #[test]
    fn flags_hexadecimal_key() {
        assert_eq!(
            messages("({ 0x1: 1 });"),
            vec!["Hexadecimal number literal is not allowed here."]
        );
    }

    #[test]
    fn flags_octal_prefixed_key() {
        assert_eq!(
            messages("({ 0o12: 1 });"),
            vec!["Octal number literal is not allowed here."]
        );
    }

    #[test]
    fn flags_binary_key() {
        assert_eq!(
            messages("({ 0b1: 1 });"),
            vec!["Binary number literal in is not allowed here."]
        );
    }

    #[test]
    fn flags_short_octal_prefixed_key() {
        assert_eq!(
            messages("({ 0o1: 1 });"),
            vec!["Octal number literal is not allowed here."]
        );
    }

    #[test]
    fn flags_decimal_with_underscore() {
        assert_eq!(
            messages("({ 1_0: 1 });"),
            vec!["Number literal with underscore is not allowed here."]
        );
    }

    #[test]
    fn flags_float_exponent_with_underscore() {
        assert_eq!(
            messages(r#"({ 0.1e1_2: "ed" });"#),
            vec!["Number literal with underscore is not allowed here."]
        );
    }

    #[test]
    fn flags_float_with_underscore() {
        assert_eq!(
            messages(r#"({ 11_1.11: "ee" });"#),
            vec!["Number literal with underscore is not allowed here."]
        );
    }

    #[test]
    fn flags_hexadecimal_method_key() {
        assert_eq!(
            messages("({ 0x1() {} });"),
            vec!["Hexadecimal number literal is not allowed here."]
        );
    }

    #[test]
    fn flags_hexadecimal_getter_key() {
        assert_eq!(
            messages("({ get 0x1() { return this.a } });"),
            vec!["Hexadecimal number literal is not allowed here."]
        );
    }

    #[test]
    fn flags_hexadecimal_setter_key() {
        assert_eq!(
            messages("({ set 0x1(a) { this.a = a } });"),
            vec!["Hexadecimal number literal is not allowed here."]
        );
    }

    #[test]
    fn ignores_computed_hexadecimal_key() {
        // `[0x1]` is a computed key — a dynamic expression, not a member name.
        assert!(run_on("({ [0x1]() {} });").is_empty());
    }

    #[test]
    fn full_invalid_fixture_reports_each_bad_key() {
        let src = r#"({ 1n: 1 });
({ 0x1: 1 });
({
	0x1: 1
});
({ 0o12: 1 });
({ 0b1: 1 });
({ 0o1: 1 });
({ 1_0: 1 });
({ 0.1e1_2: "ed" });
({ 11_1.11: "ee" });
({ 0x1() {} });
({ [0x1]() {} });
({ get 0x1() { return this.a } });
({ set 0x1(a) { this.a = a } });"#;
        // 14 fixture members, but the computed `[0x1]` is skipped → 12 diagnostics
        // (Biome's invalid.js.snap reports the same 12 keys).
        assert_eq!(run_on(src).len(), 12);
    }

    // ── Biome valid.js fixtures ───────────────────────────────────────────

    #[test]
    fn allows_plain_decimal_keys() {
        assert!(run_on(r#"({ 0: "zero" });"#).is_empty());
        assert!(run_on(r#"({ 1: "one" });"#).is_empty());
    }

    #[test]
    fn allows_float_keys() {
        assert!(run_on(r#"({ 1.2: "12" });"#).is_empty());
    }

    #[test]
    fn allows_exponent_keys() {
        assert!(run_on(r#"({ 3.1e12: "12" });"#).is_empty());
        assert!(run_on(r#"({ 0.1e12: "ee" });"#).is_empty());
    }

    #[test]
    fn allows_decimal_key_with_comment() {
        assert!(run_on("({\n\t// n\n\t20: \"20\"\n});").is_empty());
    }

    #[test]
    fn allows_shorthand_and_spread() {
        assert!(run_on("({ a });").is_empty());
        assert!(run_on("({ ...a });").is_empty());
    }

    // ── Over-firing guards ────────────────────────────────────────────────

    #[test]
    fn allows_string_and_identifier_keys() {
        assert!(run_on(r#"({ "0x1": 1, foo: 2 });"#).is_empty());
    }

    #[test]
    fn allows_multiple_plain_decimal_keys() {
        assert!(run_on(r#"({ 0: 'a', 1: 'b' });"#).is_empty());
    }

    #[test]
    fn does_not_flag_numeric_value() {
        // The forbidden forms appear in the value position, not the key.
        assert!(run_on("({ a: 0x1, b: 1_000, c: 0o7, d: 1n });").is_empty());
    }
}
