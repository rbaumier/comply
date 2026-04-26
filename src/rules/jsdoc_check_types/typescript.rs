//! jsdoc/check-types — flag capitalised primitive type names in
//! JSDoc type expressions (`{String}` → `{string}`).
//!
//! The capitalised forms refer to JavaScript's wrapper constructor
//! objects (`new String("hi")`) which behave differently from the
//! primitives (`"hi"`). Using them in type positions is almost
//! always a mistake and surprises readers who expect TypeScript-like
//! behaviour.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

/// Pairs of `(bad, preferred)`. Order matters only for the error
/// message — we check each bad token as a whole word inside `{...}`.
const PREFERENCES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Symbol", "symbol"),
    ("Bigint", "bigint"),
    ("BigInt", "bigint"),
    ("Object", "object"),
];

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            let Some(type_expr) = extract_type_expr(&tag.body) else {
                continue;
            };
            for (bad, good) in PREFERENCES {
                if contains_identifier(type_expr, bad) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: tag.line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "JSDoc type `{bad}` refers to the wrapper object — use lowercase `{good}` instead."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}

/// Extract the first balanced `{...}` group from a tag body. Returns
/// the inner text (without the braces).
fn extract_type_expr(body: &str) -> Option<&str> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let bytes = trimmed.as_bytes();
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'{' {
            if depth == 0 {
                start = Some(i + 1);
            }
            depth += 1;
        } else if b == b'}' {
            depth -= 1;
            if depth == 0 {
                let s = start?;
                return Some(&trimmed[s..i]);
            }
        }
    }
    None
}

fn contains_identifier(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = hay.as_bytes();
    let n = needle.as_bytes();
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + n.len();
            let after_ok = after_idx == bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_uppercase_string() {
        let src = "/**\n * @param {String} x\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("string"));
    }

    #[test]
    fn allows_lowercase_string() {
        let src = "/**\n * @param {string} x\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_capitalised_in_description() {
        let src = "/**\n * @param {string} x - a String value\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_number_and_boolean() {
        let src = "/**\n * @param {Number} n\n * @param {Boolean} b\n */\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn handles_union_types() {
        let src = "/**\n * @param {string | Number} x\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Number"));
    }
}
