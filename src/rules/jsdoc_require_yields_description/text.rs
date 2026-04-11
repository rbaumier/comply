//! jsdoc-require-yields-description backend — `@yields` must have a description.

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

/// Skip a `{type}` annotation at the start of text, return the rest.
fn skip_type_annotation(text: &str) -> &str {
    let t = text.trim_start();
    if let Some(rest) = t.strip_prefix('{') {
        match rest.find('}') {
            Some(close) => rest[close + 1..].trim_start(),
            None => t,
        }
    } else {
        t
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            for (line_offset, line) in block.lines().enumerate() {
                let trimmed = line.trim().trim_start_matches('*').trim();
                if let Some(after) = trimmed.strip_prefix("@yields") {
                    let after = after.trim_start();
                    let rest = skip_type_annotation(after);
                    // For tags with names (like @template T), skip the name too
                    let after_name = rest
                        .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$')
                        .trim();
                    let desc = after_name.strip_prefix('-').unwrap_or(after_name).trim();
                    if desc.is_empty() {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: block_start + line_offset + 1,
                            column: 1,
                            rule_id: "jsdoc-require-yields-description".into(),
                            message: "`@yields` tag is missing a description.".into(),
                            severity: Severity::Warning,
                        });
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
    fn flags_missing_description() {
        let src = r#"
/**
 * @yields {number}
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_tag_with_description() {
        let src = r#"
/**
 * @yields {number} - a detailed description here
 */
"#;
        assert!(run(src).is_empty());
    }
}
