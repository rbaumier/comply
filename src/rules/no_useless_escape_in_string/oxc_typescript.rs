//! OXC backend for no-useless-escape-in-string.
//!
//! Ports Biome's `noUselessEscapeInString`. An escape is *useless* when the
//! backslash precedes a character that has no special meaning in that string
//! context: removing the backslash leaves the string value unchanged. Such
//! escapes only confuse a reader.
//!
//! The classification is byte-exact with Biome and runs over the RAW literal
//! source (backslashes verbatim), not the decoded value. Necessary escapes:
//! `\b \f \n \r \t \v`, `\\`, `\^`, the hex/unicode/octal starters
//! `\x \u \0..\7`, the U+2028 / U+2029 line/paragraph separators, and the
//! delimiter quote of the enclosing literal. In template literals two extra
//! escapes are necessary: `\${` (escapes an interpolation start) and `$\{`
//! (the `\` after a `$`). Everything else escaped is useless.
//!
//! JSX attribute strings and tagged-template literals are ignored, matching
//! Biome (JSX strings keep raw text; a tag may rely on `String.raw`-style
//! semantics).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StringLiteral(lit) => {
                // JSX attribute strings are kept verbatim by the runtime, so an
                // escape there is not "useless"; Biome excludes them via a
                // distinct `JsxString` node, oxc reuses `StringLiteral`.
                if matches!(
                    semantic.nodes().parent_node(node.id()).kind(),
                    AstKind::JSXAttribute(_)
                ) {
                    return;
                }
                let start = lit.span.start as usize;
                // Raw slice keeps the enclosing quotes; the first byte is the
                // delimiter, which is exactly what Biome passes as `quote`.
                let raw = &ctx.source[start..lit.span.end as usize];
                let Some(quote) = raw.bytes().next() else {
                    return;
                };
                if let Some(index) = next_useless_escape(raw, quote) {
                    push_diagnostic(raw, start, index, ctx, diagnostics);
                }
            }
            AstKind::TemplateLiteral(tpl) => {
                // Tagged templates may rely on the raw escape (e.g. `String.raw`),
                // so escapes inside them are never useless.
                if matches!(
                    semantic.nodes().parent_node(node.id()).kind(),
                    AstKind::TaggedTemplateExpression(_)
                ) {
                    return;
                }
                // Only the static chunks are scanned; the `${...}` expressions
                // are separate AST nodes (and would be visited on their own).
                for quasi in &tpl.quasis {
                    let raw = quasi.value.raw.as_str();
                    let start = quasi.span.start as usize;
                    if let Some(index) = next_useless_escape(raw, b'`') {
                        push_diagnostic(raw, start, index, ctx, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_diagnostic(
    raw: &str,
    raw_byte_start: usize,
    index: usize,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // `index` points at the backslash; the escaped character starts one byte
    // later. Report the escaped character, like Biome.
    let escaped_char_offset = raw_byte_start + index + 1;
    let (line, column) = byte_offset_to_line_col(ctx.source, escaped_char_offset);
    let escaped_char = raw[index + 1..].chars().next();
    let message = match escaped_char {
        Some(c) => format!(
            "The character `{c}` doesn't need to be escaped. Only quotes that enclose the string \
             and special characters need to be escaped."
        ),
        None => "The character doesn't need to be escaped. Only quotes that enclose the string \
                 and special characters need to be escaped."
            .to_string(),
    };
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message,
        severity: Severity::Warning,
        span: None,
    });
}

/// Returns the byte index of the backslash of the first useless escape in `str`,
/// or `None` when every escape is necessary. `quote` is the delimiter of the
/// enclosing literal (`'`, `"`, or `` ` `` for template chunks).
///
/// Byte-exact port of Biome's `next_useless_escape`.
fn next_useless_escape(str: &str, quote: u8) -> Option<usize> {
    let bytes = str.as_bytes();
    let mut it = str.bytes().enumerate();
    while let Some((i, c)) = it.next() {
        if c != b'\\' {
            continue;
        }
        let Some((_, c)) = it.next() else {
            continue;
        };
        match c {
            // Meaningful escaped characters: control chars, octal, hex/unicode
            // starters, and the caret.
            b'^' | b'\r' | b'\n' | b'0'..=b'7' | b'\\' | b'b' | b'f' | b'n' | b'r' | b't'
            | b'u' | b'v' | b'x' => {}
            // `\${` is a valid escape only in template literals, producing a
            // literal `${`. Peek without consuming so a following escape is
            // still checked.
            b'$' => {
                if !(quote == b'`' && matches!(it.clone().next(), Some((_, b'{')))) {
                    return Some(i);
                }
            }
            // `\{` is a valid escape only in template literals when the
            // preceding character is `$` (i.e. `$\{`).
            b'{' => {
                if !(quote == b'`' && i > 0 && bytes[i - 1] == b'$') {
                    return Some(i);
                }
            }
            // Preserve escaping of U+2028 / U+2029 (bytes E2 80 A8/A9).
            0xE2 => {
                if !(matches!(it.next(), Some((_, 0x80)))
                    && matches!(it.next(), Some((_, 0xA8 | 0xA9))))
                {
                    return Some(i);
                }
            }
            // The enclosing quote can be escaped; anything else is useless.
            _ => {
                if c != quote {
                    return Some(i);
                }
            }
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

    // --- unit tests for the byte-exact classifier (Biome's own assertions) ---

    #[test]
    fn classifier_matches_biome_unit_tests() {
        assert_eq!(next_useless_escape(r"\n", b'"'), None);
        assert_eq!(next_useless_escape(r"\'", b'"'), Some(0));
        assert_eq!(next_useless_escape("\\\u{2027}", b'"'), Some(0));
        assert_eq!(next_useless_escape("\\\u{2028}", b'"'), None);
        assert_eq!(next_useless_escape("\\\u{2029}", b'"'), None);
        assert_eq!(next_useless_escape("\\\u{2030}", b'"'), Some(0));
    }

    // --- Necessary escapes: must NOT fire ---

    #[test]
    fn allows_newline_escape() {
        assert!(run_on(r#"const s = "\n";"#).is_empty());
    }

    #[test]
    fn allows_control_char_escapes() {
        assert!(run_on(r#"const s = "\b\f\n\r\t\v";"#).is_empty());
    }

    #[test]
    fn allows_backslash_escape() {
        assert!(run_on(r#"const s = "\\";"#).is_empty());
    }

    #[test]
    fn allows_caret_escape() {
        assert!(run_on(r#"const s = "\^";"#).is_empty());
    }

    #[test]
    fn allows_hex_escape() {
        assert!(run_on(r#"const s = "\x41";"#).is_empty());
    }

    #[test]
    fn allows_unicode_escape() {
        assert!(run_on(r#"const s = "A";"#).is_empty());
    }

    #[test]
    fn allows_unicode_brace_escape() {
        assert!(run_on(r#"const s = "\u{1F600}";"#).is_empty());
    }

    #[test]
    fn allows_octal_escape() {
        assert!(run_on(r#"const s = "\0\1\7";"#).is_empty());
    }

    #[test]
    fn allows_null_then_escaped_single_quote_in_single_quoted() {
        // Biome valid fixture: '\0\'' — null escape and the matching-quote escape.
        assert!(run_on(r#"const s = '\0\'';"#).is_empty());
    }

    #[test]
    fn allows_escaped_double_quote_in_double_quoted() {
        // Biome valid fixture: "\n\"" — the double quote is the delimiter.
        assert!(run_on(r#"const s = "\n\"";"#).is_empty());
    }

    #[test]
    fn allows_unicode_separator_escapes() {
        assert!(run_on("const s = \"\\\u{2028}\\\u{2029}\";").is_empty());
    }

    // --- Useless escapes: must fire ---

    #[test]
    fn flags_useless_alpha_escape_double_quoted() {
        let d = run_on(r#"const s = "\a";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_alpha_escape_single_quoted() {
        let d = run_on(r#"const s = '\a';"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_double_quote_in_single_quoted() {
        // Biome invalid fixture: '\"' — the double quote is not the delimiter.
        let d = run_on(r#"const s = '\"';"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_single_quote_in_double_quoted() {
        // Biome invalid fixture: "\'" — the single quote is not the delimiter.
        let d = run_on(r#"const s = "\'";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_in_object_key() {
        // Biome JsLiteralMemberName fixture: { '\a': 0 } — quoted key.
        let d = run_on(r#"const o = { '\a': 0 };"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_after_multibyte_char() {
        // Biome invalid fixture: "😀\😀" — the escaped 😀 is useless.
        let d = run_on("const s = \"😀\\😀\";");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_only_first_useless_escape_per_literal() {
        // Biome's classifier reports the first useless escape only.
        let d = run_on(r#"const s = "\a\b\c";"#);
        assert_eq!(d.len(), 1);
    }

    // --- Template literals ---

    #[test]
    fn allows_escaped_backtick_in_template() {
        // Biome valid fixture: `\`` — the backtick is the delimiter.
        assert!(run_on("const s = `\\``;").is_empty());
    }

    #[test]
    fn allows_escaped_interpolation_start_in_template() {
        // Biome valid fixtures: `\${`, `\${}`.
        assert!(run_on("const s = `\\${`;").is_empty());
        assert!(run_on("const s = `\\${}`;").is_empty());
    }

    #[test]
    fn allows_escaped_brace_after_dollar_in_template() {
        // Biome valid fixtures: `$\{`, `$\{}`.
        assert!(run_on("const s = `$\\{`;").is_empty());
        assert!(run_on("const s = `$\\{}`;").is_empty());
    }

    #[test]
    fn flags_useless_escape_in_template_chunk() {
        // Biome invalid fixture: ` test ${1} \a` — the \a chunk escape is useless.
        let d = run_on("const s = ` test ${1} \\a`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_dollar_without_brace_in_template() {
        // Biome invalid fixture: `\$x` — `\$` not followed by `{` is useless.
        let d = run_on("const s = `\\$x`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_after_escaped_interpolation_start() {
        // Biome invalid fixture: `\${\a` — `\${` is valid, `\a` is useless.
        let d = run_on("const s = `\\${\\a`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_after_escaped_interpolation_start_with_brace() {
        // Biome invalid fixture: `\${} \a` — `\${` valid, `\a` useless.
        let d = run_on("const s = `\\${} \\a`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_brace_without_dollar_in_template() {
        // Biome invalid fixture: `a\{` — `\{` not preceded by `$` is useless.
        let d = run_on("const s = `a\\{`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_after_dollar_escaped_brace() {
        // Biome invalid fixture: `$\{\a` — `$\{` valid, `\a` useless.
        let d = run_on("const s = `$\\{\\a`;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_useless_escape_after_dollar_escaped_brace_with_close() {
        // Biome invalid fixture: `$\{} \a` — `$\{` valid, `\a` useless.
        let d = run_on("const s = `$\\{} \\a`;");
        assert_eq!(d.len(), 1);
    }

    // --- Ignored contexts ---

    #[test]
    fn ignores_tagged_template() {
        // Biome valid fixture: tagged`\a` — tagged templates are ignored.
        assert!(run_on("const s = tagged`\\a`;").is_empty());
    }

    #[test]
    fn ignores_tagged_template_with_interpolation() {
        // Biome valid fixture: tagged` test ${1} \a`.
        assert!(run_on("const s = tagged` test ${1} \\a`;").is_empty());
    }

    #[test]
    fn ignores_jsx_attribute_string() {
        // Biome valid.jsx fixture: <div attr="str\a"/> — JSX strings are ignored.
        assert!(run_jsx(r#"const x = <div attr="str\a"/>;"#).is_empty());
    }
}
