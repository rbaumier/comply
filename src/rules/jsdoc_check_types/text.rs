//! jsdoc-check-types backend — prefer lowercase primitive types.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const BAD_TYPES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Number", "number"),
    ("Boolean", "boolean"),
    ("Object", "object"),
    ("Symbol", "symbol"),
    ("Undefined", "undefined"),
    ("Null", "null"),
    ("Void", "void"),
];

/// Extract JSDoc comment blocks from source. Returns (start_line_0based, text).
fn extract_jsdoc_blocks(source: &str) -> Vec<(usize, &str)> {
    let mut blocks = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*' {
            let start = i;
            let start_line = source[..start].matches('\n').count();
            // find closing */
            if let Some(end_rel) = source[i + 3..].find("*/") {
                let end = i + 3 + end_rel + 2;
                blocks.push((start_line, &source[start..end]));
                i = end;
            } else {
                break;
            }
        } else {
            i += 1;
        }
    }
    blocks
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            for (line_offset, line) in block.lines().enumerate() {
                let trimmed = line.trim().trim_start_matches('*').trim();
                // Look for type annotations in { }
                let mut search = trimmed;
                while let Some(open) = search.find('{') {
                    if let Some(close) = search[open..].find('}') {
                        let type_expr = &search[open + 1..open + close];
                        for &(bad, good) in BAD_TYPES {
                            if type_expr.split(|c: char| !c.is_alphanumeric()).any(|w| w == bad) {
                                diagnostics.push(Diagnostic {
                                    path: ctx.path.to_path_buf(),
                                    line: block_start + line_offset + 1,
                                    column: 1,
                                    rule_id: "jsdoc-check-types".into(),
                                    message: format!("Use `{good}` instead of `{bad}` in JSDoc type."),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                        search = &search[open + close + 1..];
                    } else {
                        break;
                    }
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::rules::backend::CheckCtx;

    fn run(source: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new("t.ts"), source);
        Check.check(&ctx)
    }

    #[test]
    fn flags_uppercase_type() {
        let src = r#"
/**
 * @param {String} name
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("string"));
    }

    #[test]
    fn allows_lowercase_type() {
        let src = r#"
/**
 * @param {string} name
 */
"#;
        assert!(run(src).is_empty());
    }
}
