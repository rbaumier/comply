//! jsdoc-valid-types backend — type expressions must be syntactically valid.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

/// Check if braces in a type expression are balanced.
fn braces_balanced(s: &str) -> bool {
    let mut depth_curly = 0i32;
    let mut depth_angle = 0i32;
    let mut depth_paren = 0i32;
    for c in s.chars() {
        match c {
            '{' => depth_curly += 1,
            '}' => { depth_curly -= 1; if depth_curly < 0 { return false; } }
            '<' => depth_angle += 1,
            '>' => { depth_angle -= 1; if depth_angle < 0 { return false; } }
            '(' => depth_paren += 1,
            ')' => { depth_paren -= 1; if depth_paren < 0 { return false; } }
            _ => {}
        }
    }
    depth_curly == 0 && depth_angle == 0 && depth_paren == 0
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            for (line_offset, line) in block.lines().enumerate() {
                let trimmed = line.trim().trim_start_matches('*').trim();
                // Extract type annotations between { }
                let mut search = trimmed;
                while let Some(open) = search.find("{") {
                    if let Some(close) = search[open..].find("}") {
                        let type_expr = &search[open + 1..open + close];
                        if !type_expr.is_empty() && !braces_balanced(type_expr) {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: block_start + line_offset + 1,
                                column: 1,
                                rule_id: "jsdoc-valid-types".into(),
                                message: "Malformed JSDoc type expression — unbalanced braces.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                        search = &search[open + close + 1..];
                    } else {
                        // Unclosed brace
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: block_start + line_offset + 1,
                            column: 1,
                            rule_id: "jsdoc-valid-types".into(),
                            message: "Unclosed `{` in JSDoc type expression.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
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
    fn flags_unclosed_brace() {
        let src = "/**\n * @param {string name\n */";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unclosed"));
    }

    #[test]
    fn allows_valid_types() {
        let src = r#"
/**
 * @param {string} name
 * @returns {number}
 */
"#;
        assert!(run(src).is_empty());
    }
}
