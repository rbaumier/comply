//! OxcCheck backend for regex-prefer-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Byte length of the escape sequence starting at `pattern[i]` (`\`), so the
/// whole sequence becomes one token. Numeric escapes (`\uXXXX`, `\u{...}`,
/// `\xHH`) are consumed whole so their hex digits never tokenize individually
/// and form phantom repetition runs; any other escape is `\` plus the next
/// UTF-8 char. Caller guarantees `i + 1 < pattern.len()`.
fn escape_len(pattern: &str, i: usize) -> usize {
    let bytes = pattern.as_bytes();
    if bytes[i + 1] == b'u' {
        if bytes.get(i + 2) == Some(&b'{') {
            let mut j = i + 3;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                j += 1;
            }
            return j - i;
        }
        if i + 6 <= bytes.len() && bytes[i + 2..i + 6].iter().all(u8::is_ascii_hexdigit) {
            return 6;
        }
    } else if bytes[i + 1] == b'x'
        && i + 4 <= bytes.len()
        && bytes[i + 2..i + 4].iter().all(u8::is_ascii_hexdigit)
    {
        return 4;
    }
    1 + pattern[i + 1..].chars().next().map_or(1, |c| c.len_utf8())
}

/// Tokenize a regex pattern into elements (single chars or escape
/// sequences like `\d`). Character classes `[...]`, groups `(` `)`,
/// alternation `|`, and `{m,n}` quantifiers are emitted as opaque
/// tokens so they never participate in repetition runs.
fn tokenize(pattern: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let len = escape_len(pattern, i);
            tokens.push(&pattern[i..i + len]);
            i += len;
        } else if bytes[i] == b'[' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b']' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else if bytes[i] == b'(' || bytes[i] == b')' || bytes[i] == b'|' {
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        } else if bytes[i] == b'{' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else {
            let ch_len = pattern[i..].chars().next().map_or(1, |c| c.len_utf8());
            tokens.push(&pattern[i..i + ch_len]);
            i += ch_len;
        }
    }
    tokens
}

/// A single literal character token (letter, digit, `-`, `/`, `_`, …) — i.e. NOT
/// a metacharacter (`(` `)` `|` `^` `$` `.` `?` `+` `*`), an escape (`\d`), a
/// char class `[...]`, or a `{m,n}` quantifier. Used to detect when a repeated
/// run is embedded inside a longer literal word.
fn is_literal_char_token(tok: &str) -> bool {
    tok.chars().count() == 1
        && !matches!(tok, "(" | ")" | "|" | "^" | "$" | "." | "?" | "+" | "*")
        && !tok.starts_with('\\')
        && !tok.starts_with('[')
        && !tok.starts_with('{')
}

/// True when the pattern contains a STANDALONE run of 3+ identical repeatable
/// tokens. A run is suppressed when it is a literal-word fragment — a repeated
/// literal char glued to a literal char on BOTH sides (e.g. `www` in
/// `x-www-form-urlencoded`), where a quantifier rewrite would be unreadable.
fn has_repeated_tokens(pattern: &str) -> bool {
    let tokens = tokenize(pattern);
    let len = tokens.len();
    let mut start = 0;
    while start < len {
        let tok = tokens[start];
        let repeatable = !matches!(tok, "(" | ")" | "|" | "?" | "+" | "*" | "^" | "$" | ".")
            && !tok.starts_with('{')
            && !tok.starts_with('[');

        let mut end = start;
        while end + 1 < len && tokens[end + 1] == tok {
            end += 1;
        }

        if repeatable && end - start + 1 >= 3 {
            let embedded = is_literal_char_token(tok)
                && start > 0
                && is_literal_char_token(tokens[start - 1])
                && end + 1 < len
                && is_literal_char_token(tokens[end + 1]);
            if !embedded {
                return true;
            }
        }

        start = end + 1;
    }
    false
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
        let AstKind::RegExpLiteral(regex) = node.kind() else {
            return;
        };

        let pattern = regex.regex.pattern.text.as_str();
        if !has_repeated_tokens(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Repeated identical pattern in regex \u{2014} use a quantifier like `a{3}` or `\\d{4}`."
                .into(),
            severity: Severity::Error,
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
    fn flags_repeated_chars() {
        assert_eq!(run_on("const re = /aaa/;").len(), 1);
    }

    #[test]
    fn flags_repeated_escape() {
        assert_eq!(run_on(r#"const re = /\d\d\d\d/;"#).len(), 1);
    }

    #[test]
    fn allows_www_in_mime_literal() {
        assert!(run_on(r#"const ct = /^application\/x-www-form-urlencoded$/;"#).is_empty());
    }

    #[test]
    fn allows_www_in_content_type_union() {
        assert!(run_on(
            r#"const re = /^\b(application\/x-www-form-urlencoded|multipart\/form-data|text\/plain)\b/i;"#
        )
        .is_empty());
    }

    #[test]
    fn allows_bare_embedded_www() {
        assert!(run_on(r#"const re = /x-www-form/;"#).is_empty());
    }

    #[test]
    fn flags_run_touching_left_edge() {
        assert_eq!(run_on("const re = /aaab/;").len(), 1);
    }

    #[test]
    fn flags_run_touching_right_edge() {
        assert_eq!(run_on("const re = /xaaa/;").len(), 1);
    }

    #[test]
    fn allows_two_chars() {
        assert!(run_on("const re = /aa/;").is_empty());
    }

    #[test]
    fn allows_quantifier_already() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        assert!(run_on(r#"const u = "http://a/aaa/b";"#).is_empty());
    }

    #[test]
    fn no_panic_on_multibyte_chars() {
        assert!(run_on(r#"const re = /cabinets vétérinaires/;"#).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        assert!(run_on(r#"import X from "@tanstack/react-query";"#).is_empty());
    }

    #[test]
    fn allows_unicode_escape_with_repeated_hex_digits() {
        // \uFFFE tokenizes as one token, not as \u + F + F + F + E.
        assert!(run_on(r#"const re = /\uFFFE(.)/g;"#).is_empty());
    }

    #[test]
    fn allows_unicode_escape_fffd() {
        assert!(run_on(r#"const re = /\uFFFD([A-E])/g;"#).is_empty());
    }

    #[test]
    fn allows_braced_unicode_escape_repeated_hex() {
        assert!(run_on(r#"const re = /\u{FFFFF}/u;"#).is_empty());
    }

    #[test]
    fn allows_hex_escape_repeated_digits() {
        // \xFF is one token; the trailing F does not form a 3-run with split hex.
        assert!(run_on(r#"const re = /\xFFF/;"#).is_empty());
    }

    #[test]
    fn flags_repeated_identical_unicode_escapes() {
        // Atomic tokenization, not blanket escape suppression: three identical
        // A escapes are a genuine repeat and still fire.
        assert_eq!(run_on(r#"const re = /\u0041\u0041\u0041/;"#).len(), 1);
    }

    #[test]
    fn allows_truncated_or_malformed_escapes_without_panic() {
        // EOF after \u / \x, a non-hex digit, and an unclosed \u{ must each
        // tokenize without panicking and produce no diagnostic.
        assert!(run_on(r#"const re = /\u/;"#).is_empty());
        assert!(run_on(r#"const re = /\x/;"#).is_empty());
        assert!(run_on(r#"const re = /\xG/;"#).is_empty());
        assert!(run_on(r#"const re = /\u{FFFF/;"#).is_empty());
    }
}
