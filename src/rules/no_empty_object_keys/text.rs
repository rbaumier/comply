//! Flags object members whose key is empty or whitespace-only.
//!
//! A scan (rather than `serde_json`) keeps the rule working on the `.json`
//! files that carry comments or trailing commas (tsconfig, editor settings),
//! which strict JSON parsing would reject outright.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// One string literal found while scanning: where it starts (byte offset and
/// 0-based line) and its unescaped content.
struct StringToken {
    byte_offset: usize,
    line: usize,
    value: String,
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        empty_key_offsets(ctx.source)
            .into_iter()
            .map(|(byte_offset, line)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: line + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Unexpected empty object key.".to_string(),
                severity: Severity::Warning,
                span: Some((byte_offset, 0)),
            })
            .collect()
    }
}

/// Byte offset and 0-based line of each object key that is empty after
/// unescaping and trimming whitespace.
fn empty_key_offsets(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut line = 0usize;
    let mut i = 0usize;
    // The most recent string literal not yet resolved as key-or-value.
    let mut pending: Option<StringToken> = None;
    let mut hits = Vec::new();

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'"' => {
                let start = i;
                let start_line = line;
                let (value, end) = read_string(bytes, i, &mut line);
                pending = Some(StringToken {
                    byte_offset: start,
                    line: start_line,
                    value,
                });
                i = end;
            }
            b':' => {
                // The string before the colon was a key.
                if let Some(tok) = pending.take()
                    && tok.value.trim().is_empty()
                {
                    hits.push((tok.byte_offset, tok.line));
                }
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            // Any other significant token means the pending string was a
            // value (or array element), not a key.
            b if !b.is_ascii_whitespace() => {
                pending = None;
                i += 1;
            }
            _ => i += 1,
        }
    }

    hits
}

/// Read a JSON string starting at the opening quote `bytes[start] == b'"'`.
/// Returns the unescaped content and the byte offset just past the closing
/// quote, advancing `line` for any newline consumed inside the literal.
fn read_string(bytes: &[u8], start: usize, line: &mut usize) -> (String, usize) {
    let mut value = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                break;
            }
            b'\\' if i + 1 < bytes.len() => {
                let (decoded, consumed) = decode_escape(bytes, i + 1);
                value.push(decoded);
                i += 1 + consumed;
            }
            b'\n' => {
                *line += 1;
                value.push('\n');
                i += 1;
            }
            b => {
                value.push(b as char);
                i += 1;
            }
        }
    }
    (value, i)
}

/// Decode the escape sequence whose body starts at `bytes[i]` (the char after
/// the backslash). Returns the decoded char and how many bytes were consumed.
/// Only `\n`, `\t`, `\r`, `\f`, `\b` map to whitespace; everything else is
/// rendered as a placeholder non-whitespace char so the key won't trim empty.
fn decode_escape(bytes: &[u8], i: usize) -> (char, usize) {
    match bytes[i] {
        b'n' => ('\n', 1),
        b't' => ('\t', 1),
        b'r' => ('\r', 1),
        b'f' => ('\u{0c}', 1),
        b'b' => ('\u{08}', 1),
        b'u' => {
            let hex_end = (i + 5).min(bytes.len());
            let hex = std::str::from_utf8(&bytes[i + 1..hex_end]).ok();
            let decoded = hex
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .and_then(char::from_u32)
                .unwrap_or('\u{fffd}');
            (decoded, hex_end - i)
        }
        // `\"`, `\\`, `\/` and any other escaped char are themselves.
        other => (other as char, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn check(content: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new("/data.json"), content);
        Check.check(&ctx)
    }

    // --- Biome invalid.json fixture: every key is empty or whitespace-only ---

    #[test]
    fn flags_every_whitespace_key_from_biome_invalid_fixture() {
        let json = "{\n  \"\": \"another value\",\n  \" \": \"space as key\",\n  \"\\t\": \"tab as key\",\n  \"\\n\": \"newline as key\",\n  \"\\n\\n\\n\": \"multi newline as key\"\n}";
        let diags = check(json);
        assert_eq!(diags.len(), 5);
    }

    #[test]
    fn flags_empty_string_key() {
        let diags = check(r#"{"": "v"}"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_single_space_key() {
        let diags = check(r#"{" ": "v"}"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_tab_escape_key() {
        let diags = check(r#"{"\t": "v"}"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_multi_newline_escape_key() {
        let diags = check(r#"{"\n\n\n": "v"}"#);
        assert_eq!(diags.len(), 1);
    }

    // --- Biome valid.json fixture: keys with surrounding whitespace but real content ---

    #[test]
    fn allows_keys_from_biome_valid_fixture() {
        let json = "{\n  \"key1\": \"value1\",\n  \"key2\": {\n    \"nestedKey\": \"nested value\",\n    \"   nestedKey \\n\\t  \": \"nested value\"\n  }\n}";
        let diags = check(json);
        assert!(diags.is_empty(), "got {diags:?}");
    }

    #[test]
    fn allows_plain_key() {
        let diags = check(r#"{"name": "v"}"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_key_padded_with_whitespace() {
        let diags = check(r#"{"  padded  ": "v"}"#);
        assert!(diags.is_empty());
    }

    // --- Over-firing guards: empty values / array elements are not keys ---

    #[test]
    fn allows_empty_string_value() {
        let diags = check(r#"{"name": ""}"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_whitespace_string_value() {
        let diags = check(r#"{"name": "  "}"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_empty_string_array_element() {
        let diags = check(r#"{"list": ["", " ", "\t"]}"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_only_the_empty_key_in_mixed_object() {
        let diags = check(r#"{"good": "v", "": "bad", "alsoGood": "v"}"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_key_in_nested_object() {
        let diags = check(r#"{"outer": {"": "v"}}"#);
        assert_eq!(diags.len(), 1);
    }

    // --- JSONC tolerance (comments / trailing commas that serde_json rejects) ---

    #[test]
    fn flags_empty_key_alongside_comments() {
        let json = "{\n  // a comment\n  \"\": \"v\", /* inline */\n}";
        let diags = check(json);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_treat_colon_in_string_value_as_key_boundary() {
        let diags = check(r#"{"url": "http://example.com"}"#);
        assert!(diags.is_empty());
    }

    #[test]
    fn reports_correct_line() {
        let json = "{\n  \"ok\": \"v\",\n  \"\": \"bad\"\n}";
        let diags = check(json);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }
}
