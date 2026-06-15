//! Flags JSON documents whose top-level value is a bare literal (string,
//! number, boolean, or `null`) rather than an object `{...}` or array `[...]`.
//!
//! A scan (rather than `serde_json`) keeps the rule working on the `.json`
//! files that carry comments (tsconfig, editor settings), which strict JSON
//! parsing would reject outright. The scanner locates the first significant
//! top-level token, skipping a leading BOM, whitespace, and JSONC line (`//`)
//! and block (`/* */`) comments; if that token is not `{` or `[`, the whole
//! literal value is reported.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(literal) = top_level_literal(ctx.source) else {
            return Vec::new();
        };

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: literal.line + 1,
            column: literal.column + 1,
            rule_id: super::META.id.into(),
            message: "Expected the top-level value to be an array or object.".to_string(),
            severity: Severity::Warning,
            span: Some((literal.byte_offset, literal.byte_len)),
        }]
    }
}

/// Location of a flagged top-level literal: byte offset and length of the whole
/// value, plus its 0-based line and column for the human-friendly anchor.
struct Literal {
    byte_offset: usize,
    byte_len: usize,
    line: usize,
    column: usize,
}

/// Returns the top-level literal value if the document's first significant
/// token is a bare literal (anything other than `{` or `[`). Returns `None`
/// for an object/array root, or for an empty/comment-only document (nothing to
/// flag — strict JSON would reject those, but this rule only judges the root
/// value's shape).
fn top_level_literal(source: &str) -> Option<Literal> {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    let mut line = 0usize;
    let mut col = 0usize;

    // Skip a leading UTF-8 BOM (EF BB BF) — it is not part of the value.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        i = 3;
        col = 3;
    }

    // Skip whitespace and JSONC comments until the first significant byte.
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                col = 0;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                    col += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                col += 2;
                while i < bytes.len() && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/')) {
                    if bytes[i] == b'\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                    i += 1;
                }
                // Consume the closing `*/` if present.
                if i < bytes.len() {
                    i += 2;
                    col += 2;
                }
            }
            b if b.is_ascii_whitespace() => {
                i += 1;
                col += 1;
            }
            // First significant byte.
            b'{' | b'[' => return None,
            _ => {
                let byte_len = literal_len(bytes, i);
                return Some(Literal {
                    byte_offset: i,
                    byte_len,
                    line,
                    column: col,
                });
            }
        }
    }

    None
}

/// Byte length of the bare literal starting at `bytes[start]`. A string runs to
/// its closing quote (honoring backslash escapes); any other literal (number,
/// `true`/`false`/`null`, or a malformed token) runs to the next whitespace,
/// structural char, or comment start.
fn literal_len(bytes: &[u8], start: usize) -> usize {
    if bytes[start] == b'"' {
        let mut i = start + 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1 - start,
                _ => i += 1,
            }
        }
        // Unterminated string: span to end of input.
        return bytes.len() - start;
    }

    let mut i = start;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace()
            || matches!(b, b',' | b'}' | b']' | b':')
            || (b == b'/' && matches!(bytes.get(i + 1), Some(&b'/') | Some(&b'*')))
        {
            break;
        }
        i += 1;
    }
    i - start
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn check(content: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new("/data.json"), content);
        Check.check(&ctx)
    }

    // --- Biome valid fixtures: object / array roots do not fire ---

    #[test]
    fn allows_object_root() {
        let json = "{\n  \"one\": \"value\",\n  \"two\": \"value\",\n  \"three\": \"value\"\n}";
        assert!(check(json).is_empty());
    }

    #[test]
    fn allows_array_root() {
        let json = "[\n  \"one\",\n  \"two\",\n  \"three\"\n]";
        assert!(check(json).is_empty());
    }

    #[test]
    fn allows_empty_object_root() {
        assert!(check("{}").is_empty());
    }

    #[test]
    fn allows_empty_array_root() {
        assert!(check("[]").is_empty());
    }

    // --- Biome invalid fixtures: bare literal roots fire ---

    #[test]
    fn flags_string_root() {
        let diags = check("\"just a string\"\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].column, 1);
        // Span covers the whole quoted literal: `"just a string"` = 15 bytes.
        assert_eq!(diags[0].span, Some((0, 15)));
        assert_eq!(
            diags[0].message,
            "Expected the top-level value to be an array or object."
        );
    }

    #[test]
    fn flags_number_root() {
        let diags = check("42\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((0, 2)));
    }

    #[test]
    fn flags_boolean_root() {
        let diags = check("true\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((0, 4)));
    }

    #[test]
    fn flags_null_root() {
        let diags = check("null\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((0, 4)));
    }

    // --- Leading whitespace / BOM / comments still resolve to the real root ---

    #[test]
    fn flags_literal_after_leading_whitespace() {
        let diags = check("\n   42");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 4);
        assert_eq!(diags[0].span, Some((4, 2)));
    }

    #[test]
    fn allows_object_after_bom() {
        let json = "\u{feff}{\n  \"a\": 1\n}";
        assert!(check(json).is_empty());
    }

    #[test]
    fn flags_literal_after_bom() {
        // BOM is 3 bytes; the literal starts at offset 3.
        let diags = check("\u{feff}\"x\"");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((3, 3)));
    }

    #[test]
    fn allows_object_after_line_comment() {
        let json = "// a comment\n{ \"a\": 1 }";
        assert!(check(json).is_empty());
    }

    #[test]
    fn allows_array_after_block_comment() {
        let json = "/* leading\n   block comment */\n[1, 2, 3]";
        assert!(check(json).is_empty());
    }

    #[test]
    fn flags_literal_after_line_comment() {
        let diags = check("// note\n\"value\"");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 1);
    }

    #[test]
    fn flags_literal_after_block_comment() {
        let diags = check("/* note */ 3.14");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((11, 4)));
    }

    // --- Edge cases: nothing significant to judge ---

    #[test]
    fn allows_empty_document() {
        assert!(check("").is_empty());
        assert!(check("   \n  ").is_empty());
    }

    #[test]
    fn allows_comment_only_document() {
        assert!(check("// just a comment\n").is_empty());
        assert!(check("/* only a block comment */").is_empty());
    }

    #[test]
    fn string_with_braces_is_still_a_literal() {
        // A `{` inside a string literal must not be mistaken for an object root.
        let diags = check("\"{not an object}\"");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span, Some((0, 17)));
    }
}
