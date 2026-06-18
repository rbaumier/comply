//! regex-no-duplicate-chars OXC backend — visit `RegExpLiteral` nodes only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Scans a regex pattern for `[...]` character classes containing
/// duplicate single characters (e.g. `[aab]`).
fn has_duplicate_in_char_class(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let start = i + 1;
            let content_start = if start < bytes.len() && bytes[start] == b'^' {
                start + 1
            } else {
                start
            };
            let mut j = start;
            if j < bytes.len() && bytes[j] == b']' {
                j += 1;
            }
            while j < bytes.len() && bytes[j] != b']' {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                j += 1;
            }
            if j < bytes.len() {
                let content = &pattern[content_start..j];
                if class_has_duplicate_member(content) {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// Tokenizes one `[...]` class body into its members and reports whether any
/// literal member repeats. Each escape (`\uXXXX`, `\u{..}`, `\xXX`, `\p{..}`,
/// `\P{..}`, or a single-char escape like `\n`) is one opaque token, so its
/// payload bytes are never mistaken for literal members; ranges (`a-z`) are
/// skipped. Property escapes (`\p{..}`/`\P{..}`) only collide with an identical
/// property escape, never with a literal char.
fn class_has_duplicate_member(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut tokens: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let token_len = member_len(&bytes[i..]);
        let end = i + token_len;
        // A `member-member` range contributes no dedup candidate.
        if end < bytes.len() && bytes[end] == b'-' && end + 1 < bytes.len() {
            let rhs_len = member_len(&bytes[end + 1..]);
            i = end + 1 + rhs_len;
            continue;
        }
        tokens.push(&content[i..end]);
        i = end;
    }
    let len_before = tokens.len();
    tokens.sort_unstable();
    tokens.dedup();
    tokens.len() < len_before
}

/// Byte length of the class member starting at `bytes[0]`. A backslash escape
/// spans its full payload (`\u{..}`/`\uXXXX`/`\xXX`/`\p{..}`/`\P{..}` or a
/// single-char escape); a literal member spans one whole UTF-8 scalar so the
/// returned length always lands on a `char` boundary.
fn member_len(bytes: &[u8]) -> usize {
    if bytes[0] != b'\\' {
        return utf8_char_len(bytes[0]);
    }
    if bytes.len() < 2 {
        return 1;
    }
    match bytes[1] {
        b'u' if bytes.get(2) == Some(&b'{') => brace_escape_len(bytes),
        b'u' => 6.min(bytes.len()),
        b'x' => 4.min(bytes.len()),
        b'p' | b'P' if bytes.get(2) == Some(&b'{') => brace_escape_len(bytes),
        _ => 2,
    }
}

/// Byte width of the UTF-8 scalar whose leading byte is `b`.
fn utf8_char_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Byte length of a brace-delimited escape (`\u{..}`, `\p{..}`, `\P{..}`),
/// counting through the closing `}`. Falls back to the remaining length when
/// the brace is unterminated.
fn brace_escape_len(bytes: &[u8]) -> usize {
    match bytes.iter().position(|&b| b == b'}') {
        Some(close) => close + 1,
        None => bytes.len(),
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let pattern = &ctx.source[re.span.start as usize..re.span.end as usize];
        // Strip surrounding `/pattern/flags`.
        let Some(inner) = pattern.strip_prefix('/') else { return };
        let Some(last_slash) = inner.rfind('/') else { return };
        let pat = &inner[..last_slash];

        if !has_duplicate_in_char_class(pat) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Duplicate character in regex character class \u{2014} remove the redundant character.".into(),
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
    fn flags_duplicate_chars() {
        assert_eq!(run_on("const re = /[aab]/;").len(), 1);
    }

    #[test]
    fn flags_duplicate_chars_non_adjacent() {
        assert_eq!(run_on("const re = /[aba]/;").len(), 1);
    }

    #[test]
    fn allows_unique_chars() {
        assert!(run_on("const re = /[abc]/;").is_empty());
    }

    #[test]
    fn allows_ranges() {
        assert!(run_on("const re = /[a-z]/;").is_empty());
    }

    // --- #3825: escape payloads must not leak as phantom literal members. ---

    #[test]
    fn allows_distinct_unicode_escapes() {
        assert!(run_on(r"const re = /[\u2028\u2029]/;").is_empty());
    }

    #[test]
    fn allows_hex_escape_with_literals() {
        assert!(run_on(r"const re = /[\^@-\^_\x7F]/;").is_empty());
    }

    #[test]
    fn allows_distinct_property_escapes() {
        assert!(run_on(r"const re = /[\p{ID_Start}\p{ID_Continue}]/u;").is_empty());
    }

    #[test]
    fn allows_brace_unicode_escapes() {
        assert!(run_on(r"const re = /[\u{1F600}\u{1F601}]/u;").is_empty());
    }

    // --- #3825: real duplicates of escapes / properties still flag. ---

    #[test]
    fn flags_duplicate_unicode_escape() {
        assert_eq!(run_on(r"const re = /[\u2028\u2028]/;").len(), 1);
    }

    #[test]
    fn flags_duplicate_property_escape() {
        assert_eq!(run_on(r"const re = /[\p{L}\p{L}]/u;").len(), 1);
    }
}
