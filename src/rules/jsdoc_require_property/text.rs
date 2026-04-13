//! jsdoc-require-property backend — `@typedef` must document properties.

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

/// Extract tag entries from a JSDoc block. Returns (line_offset, tag_name, rest_of_line).
fn extract_tags(block: &str) -> Vec<(usize, String, String)> {
    let mut tags = Vec::new();
    for (line_offset, line) in block.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('*').trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            let tag_name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-')
                .collect();
            if !tag_name.is_empty() {
                let after = rest[tag_name.len()..].trim().to_string();
                tags.push((line_offset, tag_name, after));
            }
        }
    }
    tags
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (block_start, block) in extract_jsdoc_blocks(ctx.source) {
            let tags = extract_tags(block);
            let has_typedef = tags.iter().any(|(_, t, _)| t == "typedef");
            if !has_typedef {
                continue;
            }
            let has_property = tags.iter().any(|(_, t, _)| t == "property" || t == "prop");
            if !has_property {
                // Find the typedef line
                let typedef_line = tags
                    .iter()
                    .find(|(_, t, _)| t == "typedef")
                    .map(|(lo, _, _)| *lo)
                    .unwrap_or(0);
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: block_start + typedef_line + 1,
                    column: 1,
                    rule_id: "jsdoc-require-property".into(),
                    message: "`@typedef` is missing `@property` tags.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
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
    fn flags_typedef_without_properties() {
        let src = r#"
/**
 * @typedef Foo
 */
"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_typedef_with_properties() {
        let src = r#"
/**
 * @typedef Foo
 * @property {string} name
 */
"#;
        assert!(run(src).is_empty());
    }
}
