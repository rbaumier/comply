use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Count `\\` pairs in a string node's source text.
fn count_escaped_backslashes(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'\\' {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

/// `String.raw` reproduces a literal's text byte-for-byte; an escape sequence
/// whose cooked value differs from its source spelling cannot be converted
/// without changing the string's value. Returns true if the literal body
/// (between the delimiters) contains such an escape.
///
/// JS drops the backslash for *every* unrecognized single-backslash escape
/// (`\d` → `d`, `\8` → `8`, `\.` → `.`) and rewrites the recognized control /
/// numeric / unicode ones (`\n`, `\xNN`, `\uNNNN`, …) to a different character.
/// The only lone escapes whose cooked value equals their raw spelling are the
/// escaped backslash (`\\` → `\`) and the quote escapes (`\"`, `\'`), which
/// render as a bare quote that needs no escaping inside a backtick template.
fn has_non_raw_preservable_escape(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            i += 1;
            continue;
        }
        match bytes.get(i + 1) {
            // Lone trailing backslash: handled separately by the caller.
            None => return false,
            Some(b'\\') | Some(b'"') | Some(b'\'') => i += 2,
            Some(_) => return true,
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // The rule only fires on a literal carrying two escaped backslashes,
        // so the source must contain at least one `\\` byte pair.
        Some(&["\\\\"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::StringLiteral(lit) = node.kind() else { return };
        let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];

        // Skip strings containing backticks (can't use String.raw with backticks)
        if raw.contains('`') {
            return;
        }

        // Skip strings with interpolation patterns
        if raw.contains("${") {
            return;
        }

        // `String.raw` cannot represent a value ending in a backslash — the
        // trailing `\` escapes the closing backtick (`String.raw`\`` is a
        // syntax error). Guard against malformed spans (oxc edge case on
        // very large repos) where raw.len() < 2 would make the slice panic.
        if raw.len() < 2 {
            return;
        }
        let body = &raw[1..raw.len() - 1];
        if body.ends_with('\\') {
            return;
        }

        // `String.raw` reproduces source text verbatim, so a string carrying any
        // escape whose cooked value differs from its spelling (`\n`, `\xNN`, a
        // lone `\d`, …) would silently change value under the rewrite — skip it.
        if has_non_raw_preservable_escape(body) {
            return;
        }

        if count_escaped_backslashes(raw) >= 2 {
            let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`String.raw` should be used to avoid escaping `\\`.".into(),
                severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_multi_backslash_string() {
        // `"\d\w+"` — two escaped backslashes, String.raw is a readability win.
        assert_eq!(run_on(r#"const re = "\\d\\w+";"#).len(), 1);
    }

    // Regression for #234: a single-backslash LIKE-escape constant needs no
    // String.raw, and String.raw can't carry a trailing backslash anyway.
    #[test]
    fn ignores_single_backslash() {
        assert!(run_on(r#"const LIKE_ESCAPE = "\\";"#).is_empty());
    }

    // Regression for #234: a value ending in a backslash cannot be a String.raw
    // literal even with 2+ escapes.
    #[test]
    fn ignores_string_ending_in_backslash() {
        assert!(run_on(r#"const x = "\\d\\";"#).is_empty());
    }

    // Regression for #777: empty string and single-char strings must not panic
    // (oxc can emit spans of length 0 or 1 on very large repos like microsoft/TypeScript).
    #[test]
    fn no_panic_on_short_string() {
        assert!(run_on(r#"const a = "";"#).is_empty());
        assert!(run_on(r#"const b = "x";"#).is_empty());
    }

    // Regression for #6029: strings carrying a control/numeric/unicode escape
    // must not be rewritten to `String.raw` — raw reproduces `\n`/`\r`/… as the
    // literal two-character sequence, silently changing the value.
    #[test]
    fn ignores_control_escapes() {
        // newline + carriage-return escapes alongside escaped backslashes
        assert!(run_on(r#"const x = "(\\101\\\r\n\\102\\\r\\103)";"#).is_empty());
        // tab + vertical-tab escapes
        assert!(run_on(r#"const x = "\\a\tb\\c\v";"#).is_empty());
        // hex escape
        assert!(run_on(r#"const x = "\\x\x41\\y";"#).is_empty());
        // unicode escapes (4-digit and code-point forms)
        assert!(run_on(r#"const x = "\\a\u0041\\b";"#).is_empty());
        assert!(run_on(r#"const x = "\\a\u{1F600}\\b";"#).is_empty());
        // null / octal escape
        assert!(run_on(r#"const x = "\\a\0\\b";"#).is_empty());
        // lone unrecognized escape: JS drops the backslash (`\d` -> `d`), so
        // raw (`\a\d\b`) differs from the cooked value (`\ad\b`).
        assert!(run_on(r#"const x = "\\a\d\\b";"#).is_empty());
        assert!(run_on(r#"const x = "\\a\8\\b";"#).is_empty());
    }

    // Strings whose only escapes are value-preserving under raw must still flag.
    #[test]
    fn flags_pure_backslash_strings() {
        // regex-like double backslashes
        assert_eq!(run_on(r#"const re = "\\d+\\w+";"#).len(), 1);
        // Windows path
        assert_eq!(run_on(r#"const p = "C:\\foo\\bar";"#).len(), 1);
    }
}
