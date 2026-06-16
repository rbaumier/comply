//! OXC backend for no-octal-escape.
//!
//! Ports Biome's `noOctalEscape`. An *octal escape* is a backslash followed by
//! an octal-digit sequence (`\1`..`\7`, `\01`, `\047`, `\251`, ...). They are
//! deprecated since ECMAScript 5 and should be written as hexadecimal or
//! unicode escapes instead.
//!
//! The classification is byte-exact with Biome and runs over the RAW literal
//! source (backslashes verbatim), not the decoded value. Only the *first*
//! octal escape per literal is reported, matching Biome.
//!
//! The bare NUL escape `\0` is **not** octal: a `\0` not followed by another
//! octal digit (`\0`, `\0a`, `\08`, `\0 `) is valid and never reported. A `\0`
//! followed by another octal digit (`\00`, `\07`, `\012`) *is* an octal escape
//! and fires. Hex (`\xA9`) and unicode (`©`) escapes never fire. `\8` and
//! `\9` are not octal digits, so they never fire.
//!
//! Biome's query is `AnyJsStringLiteral` — string-literal expressions and quoted
//! object keys, both `StringLiteral` in oxc. Template literals and regex literals
//! are out of scope. JSX attribute strings are a distinct `JsxString` in Biome
//! (kept verbatim by the runtime) but reuse `StringLiteral` in oxc, so they are
//! skipped here to match Biome.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else {
            return;
        };
        // JSX attribute strings are a distinct `JsxString` node in Biome and are
        // not part of `AnyJsStringLiteral`; oxc reuses `StringLiteral`, so skip
        // them to match Biome's scope.
        if matches!(semantic.nodes().parent_node(node.id()).kind(), AstKind::JSXAttribute(_)) {
            return;
        }
        let start = lit.span.start as usize;
        // Raw slice keeps the enclosing quotes, exactly what Biome scans via
        // `text_trimmed()`.
        let raw = &ctx.source[start..lit.span.end as usize];
        if let Some(index) = next_octal_escape(raw) {
            // `index` points at the backslash; report it like Biome.
            let (line, column) = byte_offset_to_line_col(ctx.source, start + index);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Don't use deprecated octal escape sequences. Use a hexadecimal \
                          (`\\xA9`) or unicode (`\\u00A9`) escape instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Returns the byte index of the backslash of the first octal escape in `str`,
/// or `None` when there is none.
///
/// Byte-exact port of Biome's `NoOctalEscape::run`. An octal escape is `\`
/// followed by an octal digit (`0`..`7`), except the bare NUL escape `\0` (a
/// `\0` not followed by another octal digit). A backslash consumes the next
/// byte, so `\\251` is an escaped backslash followed by literal `251`, not an
/// octal escape.
fn next_octal_escape(str: &str) -> Option<usize> {
    let mut it = str.bytes().enumerate();
    while let Some((index, byte)) = it.next() {
        if byte != b'\\' {
            continue;
        }
        let Some((_, byte)) = it.next() else {
            continue;
        };
        if !matches!(byte, b'0'..=b'7') {
            continue;
        }
        // Length of the escape: `\` + first octal digit + up to 5 more octal
        // digits (Biome caps the look-ahead at 5).
        let len = 2 + it
            .clone()
            .take(5)
            .take_while(|(_, byte)| matches!(byte, b'0'..=b'7'))
            .count();
        // Ignore the non-deprecated `\0` (a lone `0` is the NUL escape, not octal).
        if byte != b'0' || len > 2 {
            return Some(index);
        }
    }
    None
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

    fn run_jsx(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    // --- unit tests for the byte-exact classifier ---

    #[test]
    fn classifier_matches_biome() {
        // bare NUL escape is valid
        assert_eq!(next_octal_escape(r#""\0""#), None);
        assert_eq!(next_octal_escape(r#""\0a""#), None);
        assert_eq!(next_octal_escape(r#""\08""#), None);
        assert_eq!(next_octal_escape(r#""\9""#), None);
        // octal escapes fire, index points at the backslash
        assert_eq!(next_octal_escape(r#""\1""#), Some(1));
        assert_eq!(next_octal_escape(r#""\01""#), Some(1));
        assert_eq!(next_octal_escape(r#""\00""#), Some(1));
        assert_eq!(next_octal_escape(r#""\251""#), Some(1));
        // escaped backslash then literal digits is not octal
        assert_eq!(next_octal_escape(r#""\\251""#), None);
    }

    // --- Valid fixtures (Biome valid.js): must NOT fire ---

    #[test]
    fn allows_hex_escape() {
        // "\x51"
        assert!(run_on(r#"const s = "\x51";"#).is_empty());
    }

    #[test]
    fn allows_escaped_backslash_then_digits() {
        // "foo \\251 bar"
        assert!(run_on(r#"const s = "foo \\251 bar";"#).is_empty());
    }

    #[test]
    fn allows_backslash_eight_and_nine() {
        // "\8" and "\9" — 8/9 are not octal digits
        assert!(run_on(r#"const s = "\8";"#).is_empty());
        assert!(run_on(r#"const s = "\9";"#).is_empty());
    }

    #[test]
    fn allows_bare_null_escape() {
        // "\0 ", "\0a"
        assert!(run_on(r#"const s = "\0 ";"#).is_empty());
        assert!(run_on(r#"const s = "\0a";"#).is_empty());
    }

    #[test]
    fn allows_plain_strings() {
        // "\\", "0", "1", "\a", "\n"
        assert!(run_on(r#"const s = "\\";"#).is_empty());
        assert!(run_on(r#"const s = "0";"#).is_empty());
        assert!(run_on(r#"const s = "1";"#).is_empty());
        assert!(run_on(r#"const s = "\a";"#).is_empty());
        assert!(run_on(r#"const s = "\n";"#).is_empty());
    }

    #[test]
    fn allows_octal_only_in_comment() {
        // "\x51" /* \01 */ — the \01 lives in a comment, not the string token
        assert!(run_on(r#"const s = "\x51" /* \01 */;"#).is_empty());
    }

    #[test]
    fn ignores_octal_in_regex_literal() {
        // /([abc]) \1/g — regex literals are out of scope (not a StringLiteral)
        assert!(run_on(r#"const r = /([abc]) \1/g;"#).is_empty());
    }

    #[test]
    fn ignores_template_literal() {
        // template literals are out of Biome's `AnyJsStringLiteral` scope
        assert!(run_on("const s = `\\251`;").is_empty());
    }

    #[test]
    fn ignores_jsx_attribute_string() {
        // JSX attribute strings are a distinct node in Biome and kept verbatim.
        assert!(run_jsx(r#"const x = <div attr="\251"/>;"#).is_empty());
    }

    // --- Invalid fixtures (Biome invalid.js): must fire ---

    #[test]
    fn flags_zero_then_octal_digit() {
        // "foo \01 bar"
        assert_eq!(run_on(r#"const s = "foo \01 bar";"#).len(), 1);
    }

    #[test]
    fn flags_triple_zero() {
        // "foo \000 bar"
        assert_eq!(run_on(r#"const s = "foo \000 bar";"#).len(), 1);
    }

    #[test]
    fn flags_three_digit_octal() {
        // "foo \377 bar"
        assert_eq!(run_on(r#"const s = "foo \377 bar";"#).len(), 1);
    }

    #[test]
    fn flags_octal_followed_by_non_octal_digit() {
        // "foo \378 bar", "foo \381 bar"
        assert_eq!(run_on(r#"const s = "foo \378 bar";"#).len(), 1);
        assert_eq!(run_on(r#"const s = "foo \381 bar";"#).len(), 1);
    }

    #[test]
    fn flags_octal_followed_by_letter() {
        // "foo \37a bar", "foo \3a1 bar", "foo \25a bar"
        assert_eq!(run_on(r#"const s = "foo \37a bar";"#).len(), 1);
        assert_eq!(run_on(r#"const s = "foo \3a1 bar";"#).len(), 1);
        assert_eq!(run_on(r#"const s = "foo \25a bar";"#).len(), 1);
    }

    #[test]
    fn flags_high_octal() {
        // "foo \751 bar", "foo \258 bar"
        assert_eq!(run_on(r#"const s = "foo \751 bar";"#).len(), 1);
        assert_eq!(run_on(r#"const s = "foo \258 bar";"#).len(), 1);
    }

    #[test]
    fn flags_octal_in_object_key() {
        // { '\31': 0 } — quoted object key is a StringLiteral
        assert_eq!(run_on(r#"const o = { '\31': 0 };"#).len(), 1);
    }

    #[test]
    fn flags_copyright_octal() {
        // Biome doc example: "Copyright \251"
        assert_eq!(run_on(r#"const foo = "Copyright \251";"#).len(), 1);
    }

    #[test]
    fn reports_only_first_octal_escape() {
        assert_eq!(run_on(r#"const s = "\1 and \2";"#).len(), 1);
    }
}
